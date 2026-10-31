//! 颜色转换与纯几何工具；当前选色器使用编码 sRGB 空间的 HSL。
//!
//! 本模块只使用 std，不进行 IO，也不引用 crate 内其他模块。对外颜色是未预乘的
//! 编码 sRGB RGBA 字节；HSV 同样针对编码后的 sRGB 通道（不是线性光 HSV，也不是 HSL）。
//! 几何坐标采用屏幕坐标（y 轴向下），色相以正上方为 0°、顺时针增大。

/// 三角中心的重心权重；退化几何和非有限输入回退时使用。
const CENTER_WEIGHTS: [f32; 3] = [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0];

/// 未预乘的 sRGB RGBA 字节颜色。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    /// 红色通道。
    pub r: u8,
    /// 绿色通道。
    pub g: u8,
    /// 蓝色通道。
    pub b: u8,
    /// Alpha 通道。
    pub a: u8,
}

impl Rgba {
    /// 不透明白色。
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// 解析 `#RRGGBB` 或 `#RRGGBBAA`（十六进制大小写均可）。
    ///
    /// 六位形式补 `alpha=FF`；空值、长度错误、非十六进制字符、名称、路径和空白一律拒绝。
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.is_empty() {
            return Err(crate::i18n::text("host-color-0004").to_string());
        }
        let bytes = text.as_bytes();
        if bytes[0] != b'#' {
            return Err(crate::i18n::text("host-color-0005").to_string());
        }
        let hex = &bytes[1..];
        if hex.len() != 6 && hex.len() != 8 {
            return Err(crate::i18n::format(
                "host-color-0001",
                &[("arg0", format!("{}", bytes.len()))],
            ));
        }
        let high = parse_hex_pair(hex, 0)?;
        let green = parse_hex_pair(hex, 2)?;
        let blue = parse_hex_pair(hex, 4)?;
        let alpha = if hex.len() == 8 {
            parse_hex_pair(hex, 6)?
        } else {
            0xFF
        };
        Ok(Self {
            r: high,
            g: green,
            b: blue,
            a: alpha,
        })
    }

    /// 规范化输出固定九字符的大写 `#RRGGBBAA`。
    pub fn format(self) -> String {
        let Self { r, g, b, a } = self;
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }

    /// 从 `[r, g, b, a]` 构造。
    pub fn from_array(value: [u8; 4]) -> Self {
        Self {
            r: value[0],
            g: value[1],
            b: value[2],
            a: value[3],
        }
    }

    /// 转换为 `[r, g, b, a]`。
    pub fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// 编码 sRGB 空间中的 HSV 颜色（H 单位为度）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsva {
    /// 色相，单位度，规范范围 `[0, 360)`。
    pub h: f32,
    /// 饱和度，规范范围 `[0, 1]`。
    pub s: f32,
    /// 明度，规范范围 `[0, 1]`。
    pub v: f32,
    /// Alpha，规范范围 `[0, 1]`。
    pub a: f32,
}

/// 编码 sRGB 空间的 HSL；只用于呈现，不进入存档。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsla {
    pub h: f32,
    pub s: f32,
    pub l: f32,
    pub a: f32,
}

impl Hsla {
    /// 无彩色保留色相；黑/白保留饱和度，避免端点编辑丢失横向位置。
    pub fn from_rgba(rgba: Rgba, last_hue: Option<f32>, last_saturation: Option<f32>) -> Self {
        let (h, _, _) = rgb_to_hsv(rgba.r, rgba.g, rgba.b);
        let max = rgba.r.max(rgba.g).max(rgba.b) as f32 / 255.0;
        let min = rgba.r.min(rgba.g).min(rgba.b) as f32 / 255.0;
        let delta = max - min;
        let l = (max + min) * 0.5;
        Self {
            h: if delta == 0.0 {
                normalize_hue(last_hue.unwrap_or(0.0))
            } else {
                h
            },
            s: if l == 0.0 || l == 1.0 {
                unit_or(last_saturation.unwrap_or(0.0), 0.0)
            } else {
                (delta / (1.0 - (2.0 * l - 1.0).abs())).clamp(0.0, 1.0)
            },
            l,
            a: rgba.a as f32 / 255.0,
        }
    }

    pub fn to_rgba(self) -> Rgba {
        let hue = normalize_hue(self.h);
        let saturation = unit_or(self.s, 0.0);
        let lightness = unit_or(self.l, 0.0);
        let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
        let sector = hue / 60.0;
        let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
        let (r, g, b) = match sector as u32 {
            0 => (chroma, second, 0.0),
            1 => (second, chroma, 0.0),
            2 => (0.0, chroma, second),
            3 => (0.0, second, chroma),
            4 => (second, 0.0, chroma),
            _ => (chroma, 0.0, second),
        };
        let offset = lightness - chroma * 0.5;
        Rgba {
            r: quantize_unit(r + offset),
            g: quantize_unit(g + offset),
            b: quantize_unit(b + offset),
            a: quantize_unit(unit_or(self.a, 1.0)),
        }
    }
}

impl Hsva {
    /// 从 RGBA 转换；灰色（S=0）时保留 `last_hue`，黑色（V=0）时保留 `last_saturation`。
    ///
    /// 首次默认 H=0、S=0。非有限的 `last_hue` 归零，非有限的 `last_saturation` 归零。
    pub fn from_rgba(rgba: Rgba, last_hue: Option<f32>, last_saturation: Option<f32>) -> Self {
        let (rgb_hue, rgb_saturation, v) = rgb_to_hsv(rgba.r, rgba.g, rgba.b);
        let fallback_hue = normalize_hue(last_hue.unwrap_or(0.0));
        let h = if rgb_saturation == 0.0 {
            fallback_hue
        } else {
            rgb_hue
        };
        let s = if v == 0.0 {
            unit_or(last_saturation.unwrap_or(0.0), 0.0)
        } else {
            rgb_saturation
        };
        Self {
            h,
            s,
            v,
            a: rgba.a as f32 / 255.0,
        }
    }

    /// 转换为 RGBA；每通道按 `round(clamp(x, 0, 1) * 255)` 量化。
    ///
    /// Alpha 不参与预乘，透明颜色的 RGB 通道会被保留。
    pub fn to_rgba(self) -> Rgba {
        let normalized = self.normalized();
        let (r, g, b) = hsv_to_rgb(normalized.h, normalized.s, normalized.v);
        Rgba {
            r,
            g,
            b,
            a: quantize_unit(normalized.a),
        }
    }

    /// 返回规范化副本：360° 归零、夹取到合法范围，非有限值回退到安全值。
    ///
    /// 回退规则：H→0、S→0、V→0、A→1。
    pub fn normalized(self) -> Self {
        Self {
            h: normalize_hue(self.h),
            s: unit_or(self.s, 0.0),
            v: unit_or(self.v, 0.0),
            a: unit_or(self.a, 1.0),
        }
    }
}

/// 把编码后的 sRGB 字节转换为 HSV。
///
/// 返回 `(h, s, v)`：H 在 `[0, 360)`，S/V 在 `[0, 1]`；这是编码 sRGB 空间中的 HSV，
/// 不是线性光 HSV 或 HSL。
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let red = r as f32 / 255.0;
    let green = g as f32 / 255.0;
    let blue = b as f32 / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    let value = max;
    let saturation = if max > 0.0 { delta / max } else { 0.0 };
    let hue = if delta == 0.0 {
        0.0
    } else {
        let sector = if max == red {
            (green - blue) / delta
        } else if max == green {
            (blue - red) / delta + 2.0
        } else {
            (red - green) / delta + 4.0
        };
        normalize_hue(sector * 60.0)
    };
    (hue, saturation, value)
}

/// 把编码 sRGB 空间中的 HSV 转换为 sRGB 字节。
///
/// H=360 归零；越界输入夹取；非有限输入回退到安全值（H→0、S/V→0），不会产生 NaN
/// 或越界通道。
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let hue = normalize_hue(h);
    let saturation = unit_or(s, 0.0);
    let value = unit_or(v, 0.0);
    let chroma = value * saturation;
    let sector = hue / 60.0;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (red, green, blue) = match sector as u32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let offset = value - chroma;
    (
        quantize_unit(red + offset),
        quantize_unit(green + offset),
        quantize_unit(blue + offset),
    )
}

/// 固定方向的三角：H 顶点为正上（纯色）、W 为左下（白）、B 为右下（黑）。
///
/// 三个顶点位于同一外接圆上；坐标使用屏幕坐标（y 轴向下）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    /// 外接圆中心。
    pub center: [f32; 2],
    /// 外接圆半径。
    pub radius: f32,
}

impl Triangle {
    fn safe_center(self) -> [f32; 2] {
        [
            finite_or_zero(self.center[0]),
            finite_or_zero(self.center[1]),
        ]
    }

    fn safe_radius(self) -> f32 {
        if self.radius.is_finite() && self.radius > 0.0 {
            self.radius
        } else {
            0.0
        }
    }

    /// 返回固定顺序的 `[H, W, B]` 顶点；零尺寸或非有限字段会被安全处理。
    pub fn vertices(self) -> [[f32; 2]; 3] {
        let [center_x, center_y] = self.safe_center();
        let radius = self.safe_radius();
        let horizontal = radius * (3.0_f32.sqrt() * 0.5);
        let half_height = radius * 0.5;
        [
            [center_x, finite_or_zero(center_y - radius)],
            [
                finite_or_zero(center_x - horizontal),
                finite_or_zero(center_y + half_height),
            ],
            [
                finite_or_zero(center_x + horizontal),
                finite_or_zero(center_y + half_height),
            ],
        ]
    }

    /// 判断点是否在三角内部或边界上；非有限点、无效尺寸返回 `false`。
    ///
    /// 零尺寸三角只把与中心完全相同的点视为命中。
    pub fn contains(self, point: [f32; 2]) -> bool {
        if !point[0].is_finite() || !point[1].is_finite() {
            return false;
        }
        if self.safe_radius() <= 0.0 {
            return point == self.safe_center();
        }
        let Some(unit_point) = self.unit_point(point) else {
            return false;
        };
        barycentric_unit(unit_point)
            .is_some_and(|weights| weights.iter().all(|value| *value >= -1.0e-6))
    }

    /// 返回重心权重 `[wH, wW, wB]`，供颜色混合使用。
    ///
    /// 内部点返回非负且和为 1 的权重；外部点回退到严格的 `nearest_weights`，因此
    /// 返回值始终是可用权重。
    pub fn weights(self, point: [f32; 2]) -> [f32; 3] {
        if let Some(weights) = self.exact_vertex_weights(point) {
            return weights;
        }
        let Some(unit_point) = self.unit_point(point) else {
            return CENTER_WEIGHTS;
        };
        match barycentric_unit(unit_point) {
            Some(raw) if raw.iter().all(|value| *value >= -1.0e-6) => convex_weights_from_f64(raw),
            _ => self.nearest_weights(point),
        }
    }

    /// 严格欧氏最近边/顶点投影对应的重心权重。
    ///
    /// 枚举三条边和三个顶点，取欧氏距离最近者，再由投影点求重心权重；不会把负权重
    /// 直接置零归一。返回值非负且和为 1。
    pub fn nearest_weights(self, point: [f32; 2]) -> [f32; 3] {
        if let Some(weights) = self.exact_vertex_weights(point) {
            return weights;
        }
        let Some(query) = self.unit_point(point) else {
            return CENTER_WEIGHTS;
        };
        let (h, w, b) = unit_triangle();
        let mut nearest = h;
        let mut nearest_distance = f64::INFINITY;
        for (start, end) in [(h, w), (w, b), (b, h)] {
            let candidate = closest_point_on_segment(start, end, query);
            let distance = distance_squared(candidate, query);
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest = candidate;
            }
        }
        for vertex in [h, w, b] {
            let distance = distance_squared(vertex, query);
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest = vertex;
            }
        }
        let Some(raw) = barycentric_unit(nearest) else {
            return CENTER_WEIGHTS;
        };
        // 投影点一定落在三角边界上；这里只修正浮点噪声造成的微小负值。
        convex_weights_from_f64(raw)
    }

    /// 按重心权重求三角内（或边界上）的坐标。
    ///
    /// 权重会先归一；非有限权重视为 0，全零时回退到中心。
    pub fn point(self, weights: [f32; 3]) -> [f32; 2] {
        let normalized = normalized_weights(weights);
        let [h, w, b] = self.vertices();
        [
            finite_or_zero(normalized[0] * h[0] + normalized[1] * w[0] + normalized[2] * b[0]),
            finite_or_zero(normalized[0] * h[1] + normalized[1] * w[1] + normalized[2] * b[1]),
        ]
    }

    /// 按重心权重混合纯色（H 顶点的当前色相）、白（W）与黑（B），输出量化 RGBA。
    ///
    /// 混合直接发生在编码 sRGB 通道上；返回值的 alpha 固定为 255，由调用者另行填写
    /// 实际透明度。权重会夹取到非负并归一，全零时回退到中心。
    pub fn color(self, hue: f32, weights: [f32; 3]) -> Rgba {
        let normalized = normalized_weights(weights);
        let (red, green, blue) = hsv_to_rgb(hue, 1.0, 1.0);
        let pure = [
            red as f32 / 255.0,
            green as f32 / 255.0,
            blue as f32 / 255.0,
        ];
        Rgba {
            r: quantize_unit(normalized[0] * pure[0] + normalized[1]),
            g: quantize_unit(normalized[0] * pure[1] + normalized[1]),
            b: quantize_unit(normalized[0] * pure[2] + normalized[1]),
            a: 255,
        }
    }

    /// 从 HSV 反算权重：`wH=V*S`、`wW=V*(1-S)`、`wB=1-V`。
    pub fn weights_for(hsva: Hsva) -> [f32; 3] {
        let normalized = hsva.normalized();
        [
            normalized.v * normalized.s,
            normalized.v * (1.0 - normalized.s),
            1.0 - normalized.v,
        ]
    }

    fn exact_vertex_weights(self, point: [f32; 2]) -> Option<[f32; 3]> {
        if self.safe_radius() <= 0.0 {
            return None;
        }
        let [h, w, b] = self.vertices();
        if point == h {
            Some([1.0, 0.0, 0.0])
        } else if point == w {
            Some([0.0, 1.0, 0.0])
        } else if point == b {
            Some([0.0, 0.0, 1.0])
        } else {
            None
        }
    }

    fn unit_point(self, point: [f32; 2]) -> Option<[f64; 2]> {
        if !point[0].is_finite() || !point[1].is_finite() {
            return None;
        }
        let radius = self.safe_radius();
        if radius <= 0.0 {
            return None;
        }
        let [center_x, center_y] = self.safe_center();
        let relative = [
            (point[0] as f64 - center_x as f64) / radius as f64,
            (point[1] as f64 - center_y as f64) / radius as f64,
        ];
        if relative[0].is_finite() && relative[1].is_finite() {
            Some(relative)
        } else {
            None
        }
    }
}

/// 以正上方为 H=0°、顺时针增大的色相角，返回 `[0, 360)`。
///
/// 坐标使用屏幕坐标（y 轴向下）；非有限输入或中心点回退到 0°。
pub fn hue_at(center: [f32; 2], point: [f32; 2]) -> f32 {
    if !center[0].is_finite()
        || !center[1].is_finite()
        || !point[0].is_finite()
        || !point[1].is_finite()
    {
        return 0.0;
    }
    let dx = point[0] - center[0];
    let dy = point[1] - center[1];
    if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
        return 0.0;
    }
    normalize_hue(dx.atan2(-dy).to_degrees())
}

/// 返回以 `center` 为圆心、`radius` 为半径、处于色相 `hue` 的圆上点。
///
/// 正上方为 H=0°、顺时针增大；坐标使用屏幕坐标（y 轴向下）。非有限圆心/半径/色相
/// 与负半径会被安全处理。
pub fn hue_point(center: [f32; 2], radius: f32, hue: f32) -> [f32; 2] {
    let center = [finite_or_zero(center[0]), finite_or_zero(center[1])];
    let radius = if radius.is_finite() && radius > 0.0 {
        radius
    } else {
        0.0
    };
    let radians = normalize_hue(hue).to_radians();
    [
        finite_or_zero(center[0] + radius * radians.sin()),
        finite_or_zero(center[1] - radius * radians.cos()),
    ]
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn parse_hex_pair(hex: &[u8], offset: usize) -> Result<u8, String> {
    let high_position = offset + 1;
    let low_position = offset + 2;
    let high = hex_value(hex[offset]).ok_or_else(|| {
        crate::i18n::format("host-color-0002", &[("arg0", high_position.to_string())])
    })?;
    let low = hex_value(hex[offset + 1]).ok_or_else(|| {
        crate::i18n::format("host-color-0003", &[("arg0", low_position.to_string())])
    })?;
    Ok(high << 4 | low)
}

fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

fn normalize_hue(hue: f32) -> f32 {
    if !hue.is_finite() {
        return 0.0;
    }
    let mut normalized = hue % 360.0;
    if normalized < 0.0 {
        normalized += 360.0;
    }
    if normalized >= 360.0 || normalized == 0.0 {
        0.0
    } else {
        normalized
    }
}

fn unit_or(value: f32, nan_fallback: f32) -> f32 {
    if value.is_nan() {
        nan_fallback
    } else {
        value.clamp(0.0, 1.0)
    }
}

fn quantize_unit(value: f32) -> u8 {
    if value.is_nan() {
        return 0;
    }
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn normalized_weights(weights: [f32; 3]) -> [f32; 3] {
    let mut safe = [0.0_f32; 3];
    for (index, value) in weights.iter().enumerate() {
        safe[index] = if value.is_finite() {
            value.max(0.0)
        } else {
            0.0
        };
    }
    normalize_weights(safe)
}

fn convex_weights_from_f64(raw: [f64; 3]) -> [f32; 3] {
    let mut converted = [0.0_f32; 3];
    for (index, value) in raw.iter().enumerate() {
        converted[index] = if value.is_finite() {
            value.max(0.0) as f32
        } else {
            0.0
        };
    }
    normalize_weights(converted)
}

fn normalize_weights(weights: [f32; 3]) -> [f32; 3] {
    let sum = weights[0] + weights[1] + weights[2];
    if !sum.is_finite() || sum <= 0.0 {
        return CENTER_WEIGHTS;
    }
    [weights[0] / sum, weights[1] / sum, weights[2] / sum]
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn unit_triangle() -> ([f64; 2], [f64; 2], [f64; 2]) {
    let horizontal = 3.0_f64.sqrt() * 0.5;
    ([0.0, -1.0], [-horizontal, 0.5], [horizontal, 0.5])
}

fn barycentric_unit(point: [f64; 2]) -> Option<[f64; 3]> {
    let (h, w, b) = unit_triangle();
    let total = cross(sub(w, h), sub(b, h));
    if total == 0.0 {
        return None;
    }
    let raw = [
        cross(sub(w, point), sub(b, point)) / total,
        cross(sub(point, h), sub(b, h)) / total,
        cross(sub(w, h), sub(point, h)) / total,
    ];
    let sum = raw[0] + raw[1] + raw[2];
    if !sum.is_finite() || sum == 0.0 {
        return None;
    }
    let weights = [raw[0] / sum, raw[1] / sum, raw[2] / sum];
    if weights.iter().all(|value| value.is_finite()) {
        Some(weights)
    } else {
        None
    }
}

fn closest_point_on_segment(start: [f64; 2], end: [f64; 2], point: [f64; 2]) -> [f64; 2] {
    let edge = sub(end, start);
    let length_squared = edge[0] * edge[0] + edge[1] * edge[1];
    if length_squared <= 0.0 || !length_squared.is_finite() {
        return start;
    }
    let offset = sub(point, start);
    let mut t = (offset[0] * edge[0] + offset[1] * edge[1]) / length_squared;
    if !t.is_finite() {
        t = 0.0;
    }
    let t = t.clamp(0.0, 1.0);
    [start[0] + t * edge[0], start[1] + t * edge[1]]
}

fn distance_squared(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let distance = dx * dx + dy * dy;
    if distance.is_finite() {
        distance
    } else {
        f64::INFINITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: f32 = 1.0e-5;

    fn assert_close(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "实际 {actual}，期望 {expected}±{tolerance}"
        );
    }

    fn assert_valid_weights(weights: [f32; 3], context: &str) {
        let sum: f32 = weights.iter().sum();
        assert!(
            (sum - 1.0).abs() <= TOLERANCE,
            "{context} 权重和应约为 1，实际 {sum}：{weights:?}"
        );
        assert!(
            weights
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0),
            "{context} 权重应有限且非负：{weights:?}"
        );
    }

    fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
        let dx = a[0] - b[0];
        let dy = a[1] - b[1];
        (dx * dx + dy * dy).sqrt()
    }

    /// 测试端的独立暴力实现：枚举三条边的最近点与三个顶点，取最近者。
    fn brute_force_nearest(triangle: Triangle, query: [f32; 2]) -> [f32; 2] {
        let [h, w, b] = triangle.vertices();
        let mut best = h;
        let mut best_distance = f32::INFINITY;
        for (start, end) in [(h, w), (w, b), (b, h)] {
            let edge = [end[0] - start[0], end[1] - start[1]];
            let length_squared = edge[0] * edge[0] + edge[1] * edge[1];
            let mut t = ((query[0] - start[0]) * edge[0] + (query[1] - start[1]) * edge[1])
                / length_squared;
            t = t.clamp(0.0, 1.0);
            let candidate = [start[0] + edge[0] * t, start[1] + edge[1] * t];
            let candidate_distance = distance(candidate, query);
            if candidate_distance < best_distance {
                best_distance = candidate_distance;
                best = candidate;
            }
        }
        for vertex in [h, w, b] {
            let vertex_distance = distance(vertex, query);
            if vertex_distance < best_distance {
                best_distance = vertex_distance;
                best = vertex;
            }
        }
        best
    }

    /// 只用于测试的固定种子 LCG；不向运行时代码开放随机。
    struct Lcg {
        state: u64,
    }

    impl Lcg {
        fn new(seed: u64) -> Self {
            Self { state: seed }
        }

        fn next_u32(&mut self) -> u32 {
            self.state = self
                .state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.state >> 33) as u32
        }

        fn next_byte(&mut self) -> u8 {
            self.next_u32() as u8
        }

        fn next_unit(&mut self) -> f32 {
            self.next_u32() as f32 / u32::MAX as f32
        }
    }

    #[test]
    fn hex_parse_format_and_validation() {
        assert_eq!(
            Rgba::parse("#FF8800").unwrap(),
            Rgba {
                r: 255,
                g: 136,
                b: 0,
                a: 255,
            }
        );
        assert_eq!(
            Rgba::parse("#0088ff80").unwrap(),
            Rgba {
                r: 0,
                g: 136,
                b: 255,
                a: 128,
            }
        );
        assert_eq!(Rgba::parse("#AbCdEf12").unwrap().format(), "#ABCDEF12");
        assert_eq!(Rgba::WHITE.format(), "#FFFFFFFF");
        for invalid in [
            "",
            "#",
            "#12345",
            "#1234567",
            "#123456789",
            "#gg0000",
            "red",
            "color:#ff0000",
            "/tmp/red.png",
            " #ff0000",
            "#ff0000 ",
            "#ff 000",
            "#ff0000\n",
        ] {
            let result = Rgba::parse(invalid);
            assert!(result.is_err(), "应拒绝非法颜色 {invalid:?}");
        }
        let parsed = Rgba::parse("#00ff00ff").unwrap();
        let formatted = parsed.format();
        assert_eq!(formatted.len(), 9);
        assert!(formatted.chars().all(|character| {
            character == '#' || character.is_ascii_digit() || character.is_ascii_uppercase()
        }));
        assert_eq!(Rgba::from_array([1, 2, 3, 4]).to_array(), [1, 2, 3, 4]);
    }

    #[test]
    fn hue_edges_and_normalization() {
        let cases = [
            (0.0_f32, 255_u8, 0_u8, 0_u8),
            (60.0, 255, 255, 0),
            (120.0, 0, 255, 0),
            (180.0, 0, 255, 255),
            (240.0, 0, 0, 255),
            (300.0, 255, 0, 255),
        ];
        for (hue, r, g, b) in cases {
            assert_eq!(hsv_to_rgb(hue, 1.0, 1.0), (r, g, b), "色相 {hue} 边界");
            let (actual_hue, saturation, value) = rgb_to_hsv(r, g, b);
            assert_close(actual_hue, hue, 1.0e-4);
            assert_close(saturation, 1.0, 1.0e-6);
            assert_close(value, 1.0, 1.0e-6);
            assert!((0.0..360.0).contains(&actual_hue));
        }
        assert_eq!(hsv_to_rgb(360.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(-360.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(720.0, 1.0, 1.0), (255, 0, 0));
        let (red_hue, _, _) = rgb_to_hsv(255, 0, 0);
        assert_eq!(red_hue, 0.0);

        for hue in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0e9, 1.0e9] {
            let normalized = Hsva {
                h: hue,
                s: f32::NAN,
                v: 2.0,
                a: -1.0,
            }
            .normalized();
            assert!(normalized.h.is_finite());
            assert!((0.0..360.0).contains(&normalized.h));
            assert!((0.0..=1.0).contains(&normalized.s));
            assert!((0.0..=1.0).contains(&normalized.v));
            assert!((0.0..=1.0).contains(&normalized.a));
        }
    }

    #[test]
    fn triangle_vertices_weights_and_hue_mapping() {
        let triangle = Triangle {
            center: [120.0, 160.0],
            radius: 90.0,
        };
        let vertices = triangle.vertices();
        let [h, w, b] = vertices;
        let expected_horizontal = 90.0 * 3.0_f32.sqrt() / 2.0;
        assert_close(h[0], 120.0, 1.0e-4);
        assert_close(h[1], 70.0, 1.0e-4);
        assert_close(w[0], 120.0 - expected_horizontal, 1.0e-4);
        assert_close(w[1], 205.0, 1.0e-4);
        assert_close(b[0], 120.0 + expected_horizontal, 1.0e-4);
        assert_close(b[1], 205.0, 1.0e-4);
        assert!(h[1] < 160.0 && w[1] > 160.0 && b[1] > 160.0);
        assert!(w[0] < 120.0 && b[0] > 120.0);
        for vertex in vertices {
            assert_close(distance(vertex, triangle.center), 90.0, 1.0e-3);
        }
        assert_close(hue_at(triangle.center, h), 0.0, 1.0e-4);
        assert_close(hue_at(triangle.center, b), 120.0, 1.0e-4);
        assert_close(hue_at(triangle.center, w), 240.0, 1.0e-4);

        let unit_weights = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        for (vertex, weights) in vertices.iter().zip(unit_weights) {
            assert_eq!(triangle.weights(*vertex), weights);
            assert_eq!(triangle.nearest_weights(*vertex), weights);
            assert_eq!(triangle.point(weights), *vertex);
        }

        for hsva in [
            Hsva {
                h: 0.0,
                s: 1.0,
                v: 1.0,
                a: 1.0,
            },
            Hsva {
                h: 210.0,
                s: 0.5,
                v: 0.5,
                a: 0.7,
            },
            Hsva {
                h: 45.0,
                s: 0.0,
                v: 1.0,
                a: 1.0,
            },
            Hsva {
                h: 300.0,
                s: 0.75,
                v: 0.0,
                a: 1.0,
            },
        ] {
            let weights = Triangle::weights_for(hsva);
            assert_valid_weights(weights, "weights_for");
            assert_close(weights[0], hsva.v * hsva.s, 1.0e-6);
            assert_close(weights[1], hsva.v * (1.0 - hsva.s), 1.0e-6);
            assert_close(weights[2], 1.0 - hsva.v, 1.0e-6);
            let value = weights[0] + weights[1];
            assert_close(value, hsva.v, 1.0e-6);
            if value > 0.0 {
                assert_close(weights[0] / value, hsva.s, 1.0e-6);
            }
            let mixed = triangle.color(hsva.h, weights);
            let direct = hsva.to_rgba();
            assert!(
                mixed.r.abs_diff(direct.r) <= 1,
                "红色通道不一致：{mixed:?} vs {direct:?}"
            );
            assert!(
                mixed.g.abs_diff(direct.g) <= 1,
                "绿色通道不一致：{mixed:?} vs {direct:?}"
            );
            assert!(
                mixed.b.abs_diff(direct.b) <= 1,
                "蓝色通道不一致：{mixed:?} vs {direct:?}"
            );
        }

        let sample = triangle.color(0.0, [0.25, 0.25, 0.5]);
        assert_eq!(
            sample,
            Rgba {
                r: 128,
                g: 64,
                b: 64,
                a: 255,
            }
        );
        let recreated = Hsva::from_rgba(sample, Some(0.0), Some(0.5));
        let back = Triangle::weights_for(recreated);
        assert_close(back[0], 0.25, 2.5e-3);
        assert_close(back[1], 0.25, 2.5e-3);
        assert_close(back[2], 0.5, 2.5e-3);
    }

    #[test]
    fn internal_sample_uses_encoded_srgb_mixing() {
        let triangle = Triangle {
            center: [0.0, 0.0],
            radius: 100.0,
        };
        let sample = triangle.color(0.0, [0.25, 0.25, 0.5]);
        assert_eq!(
            sample,
            Rgba {
                r: 128,
                g: 64,
                b: 64,
                a: 255,
            }
        );
        assert_eq!(sample.format(), "#804040FF");
        assert_eq!(triangle.color(0.0, [0.0, 0.5, 0.5]).format(), "#808080FF");
        // 线性光混合后再编码会让 50% 黑白接近 188；编码 sRGB 混合是 128。
        assert_eq!(hsv_to_rgb(0.0, 0.0, 0.5), (128, 128, 128));
        for hue in [0.0, 30.0, 120.0, 210.0, 300.0, 359.0] {
            for (saturation, value) in [(0.25, 0.75), (0.5, 0.5), (1.0, 1.0), (0.0, 1.0)] {
                let hsva = Hsva {
                    h: hue,
                    s: saturation,
                    v: value,
                    a: 1.0,
                };
                let mixed = triangle.color(hue, Triangle::weights_for(hsva));
                let direct = hsv_to_rgb(hue, saturation, value);
                assert!(
                    mixed.r.abs_diff(direct.0) <= 1,
                    "内部混合红色通道不一致：{mixed:?} vs {direct:?}"
                );
                assert!(
                    mixed.g.abs_diff(direct.1) <= 1,
                    "内部混合绿色通道不一致：{mixed:?} vs {direct:?}"
                );
                assert!(
                    mixed.b.abs_diff(direct.2) <= 1,
                    "内部混合蓝色通道不一致：{mixed:?} vs {direct:?}"
                );
            }
        }
    }

    #[test]
    fn barycentric_roundtrip_and_nearest_projection() {
        let triangle = Triangle {
            center: [37.0, -19.0],
            radius: 111.0,
        };
        let steps = 10;
        for i in 0..=steps {
            for j in 0..=(steps - i) {
                let wh = i as f32 / steps as f32;
                let ww = j as f32 / steps as f32;
                let wb = 1.0 - wh - ww;
                let point = triangle.point([wh, ww, wb]);
                assert!(triangle.contains(point));
                let recovered = triangle.weights(point);
                assert_close(recovered[0], wh, 5.0e-5);
                assert_close(recovered[1], ww, 5.0e-5);
                assert_close(recovered[2], wb, 5.0e-5);
                assert_valid_weights(recovered, "内部网格");
                let roundtrip = triangle.point(recovered);
                assert_close(roundtrip[0], point[0], 1.0e-2);
                assert_close(roundtrip[1], point[1], 1.0e-2);
            }
        }
        for weights in [
            [0.5, 0.5, 0.0],
            [0.0, 0.5, 0.5],
            [0.5, 0.0, 0.5],
            [0.25, 0.75, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ] {
            let point = triangle.point(weights);
            assert!(triangle.contains(point));
            let recovered = triangle.weights(point);
            assert_close(recovered[0], weights[0], 5.0e-5);
            assert_close(recovered[1], weights[1], 5.0e-5);
            assert_close(recovered[2], weights[2], 5.0e-5);
        }

        // 极小三角（相对中心而言）仍能完成重心往返。
        let tiny = Triangle {
            center: [0.0, 0.0],
            radius: 1.0e-30,
        };
        let tiny_point = tiny.point([0.2, 0.3, 0.5]);
        assert!(tiny.contains(tiny_point));
        let tiny_weights = tiny.weights(tiny_point);
        assert_close(tiny_weights[0], 0.2, 1.0e-3);
        assert_close(tiny_weights[1], 0.3, 1.0e-3);
        assert_close(tiny_weights[2], 0.5, 1.0e-3);

        // 靠近顶点的近退化位置。
        for scale in [1.0e-4_f32, 1.0e-3] {
            let weights = [1.0 - scale, scale * 0.5, scale * 0.5];
            let point = triangle.point(weights);
            let recovered = triangle.weights(point);
            assert_close(recovered[0], weights[0], 1.0e-4);
            assert_close(recovered[1], weights[1], 1.0e-4);
            assert_close(recovered[2], weights[2], 1.0e-4);
        }

        // 外部点：严格欧氏最近投影必须与独立暴力枚举一致，且不长于密集采样。
        let [h, w, b] = triangle.vertices();
        let mut rng = Lcg::new(0x0BAD_F00D_DEAD_BEEF);
        for _ in 0..120 {
            let angle = rng.next_unit() * std::f32::consts::TAU;
            let distance_scale = triangle.radius * (1.05 + rng.next_unit() * 3.0);
            let query = [
                triangle.center[0] + distance_scale * angle.cos(),
                triangle.center[1] + distance_scale * angle.sin(),
            ];
            let weights = triangle.nearest_weights(query);
            assert_valid_weights(weights, "外部最近投影");
            let projected = triangle.point(weights);
            let brute = brute_force_nearest(triangle, query);
            assert_close(projected[0], brute[0], 1.0e-2);
            assert_close(projected[1], brute[1], 1.0e-2);
            let projected_distance = distance(projected, query);
            let mut sampled_best = f32::INFINITY;
            for (start, end) in [(h, w), (w, b), (b, h)] {
                for step in 0..=80 {
                    let t = step as f32 / 80.0;
                    let sample = [
                        start[0] + (end[0] - start[0]) * t,
                        start[1] + (end[1] - start[1]) * t,
                    ];
                    sampled_best = sampled_best.min(distance(sample, query));
                }
            }
            for vertex in [h, w, b] {
                sampled_best = sampled_best.min(distance(vertex, query));
            }
            assert!(
                projected_distance <= sampled_best + 2.0e-3,
                "最近投影距离 {projected_distance} 应不大于密集采样 {sampled_best}"
            );
        }

        // 明显偏离三角时不能把负重心权重置零后归一。
        let far = [
            triangle.center[0] + triangle.radius * 3.0,
            triangle.center[1] + triangle.radius * 3.0,
        ];
        let strict = triangle.point(triangle.nearest_weights(far));
        let brute = brute_force_nearest(triangle, far);
        assert_close(strict[0], brute[0], 1.0e-2);
        assert_close(strict[1], brute[1], 1.0e-2);
    }

    #[test]
    fn gray_and_black_keep_presentation_hue() {
        let gray = Rgba {
            r: 128,
            g: 128,
            b: 128,
            a: 255,
        };
        let first = Hsva::from_rgba(gray, None, None);
        assert_eq!(first.h, 0.0);
        assert_eq!(first.s, 0.0);
        assert_close(first.v, 128.0 / 255.0, 1.0e-6);

        let kept = Hsva::from_rgba(gray, Some(210.0), None);
        assert_eq!(kept.h, 210.0);
        assert_eq!(kept.s, 0.0);
        assert_eq!(kept.to_rgba(), gray);

        // 灰色上改 H 不改变编码 RGB；保留的 H 供提高饱和度后使用。
        let mut changed = kept;
        changed.h = 47.0;
        assert_eq!(changed.to_rgba(), kept.to_rgba());
        assert_ne!(changed.h, kept.h);
        changed.s = 0.6;
        let colored = changed.to_rgba();
        let recovered = Hsva::from_rgba(colored, None, None);
        assert_close(recovered.h, 47.0, 2.0);
        assert_close(recovered.s, 0.6, 0.01);

        let black = Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        };
        let black_first = Hsva::from_rgba(black, None, None);
        assert_eq!(black_first.h, 0.0);
        assert_eq!(black_first.s, 0.0);
        assert_eq!(black_first.v, 0.0);
        let black_kept = Hsva::from_rgba(black, Some(300.0), Some(0.4));
        assert_eq!(black_kept.h, 300.0);
        assert_eq!(black_kept.s, 0.4);
        assert_eq!(black_kept.v, 0.0);
        assert_eq!(black_kept.to_rgba(), black);
        let black_again =
            Hsva::from_rgba(black_kept.to_rgba(), Some(black_kept.h), Some(black_kept.s));
        assert_eq!(black_again.h, 300.0);
        assert_eq!(black_again.s, 0.4);

        // 提高 V 后保留 S/H。
        let lifted = Hsva {
            v: 0.5,
            ..black_kept
        };
        let lifted_back = Hsva::from_rgba(lifted.to_rgba(), None, None);
        assert_close(lifted_back.h, 300.0, 2.0);
        assert_close(lifted_back.s, 0.4, 0.01);
    }

    #[test]
    fn alpha_roundtrip_and_transparent_rgb() {
        let transparent_red = Rgba::parse("#FF000000").unwrap();
        let hsva = Hsva::from_rgba(transparent_red, None, None);
        assert_eq!(hsva.to_rgba(), transparent_red);
        assert_eq!(hsva.to_rgba().format(), "#FF000000");
        for alpha in [0_u8, 1, 127, 128, 200, 254, 255] {
            let source = Rgba {
                r: 12,
                g: 200,
                b: 99,
                a: alpha,
            };
            let roundtrip = Hsva::from_rgba(source, Some(37.0), Some(0.2)).to_rgba();
            assert_eq!(roundtrip, source);
        }
        // alpha=false 语义属于上层；本模块不会把 A 强制改成 FF。
        let transparent = Rgba {
            r: 1,
            g: 2,
            b: 3,
            a: 0,
        };
        let hsva = Hsva::from_rgba(transparent, None, None);
        assert_eq!(hsva.a, 0.0);
        assert_eq!(hsva.to_rgba(), transparent);
        let mid = Rgba {
            r: 255,
            g: 0,
            b: 0,
            a: 128,
        };
        assert_eq!(Hsva::from_rgba(mid, None, None).to_rgba(), mid);
        assert_eq!(
            Hsva {
                h: 120.0,
                s: 1.0,
                v: 1.0,
                a: 0.0,
            }
            .to_rgba()
            .format(),
            "#00FF0000"
        );
    }

    #[test]
    fn non_finite_and_degenerate_inputs_are_safe() {
        for hue in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0e30, -1.0e30] {
            // S=NaN 回退为 0，V=+∞ 夹取为 1，因此是纯白。
            let (r, g, b) = hsv_to_rgb(hue, f32::NAN, f32::INFINITY);
            assert_eq!((r, g, b), (255, 255, 255));
            let normalized = Hsva {
                h: hue,
                s: f32::NAN,
                v: f32::NEG_INFINITY,
                a: f32::NAN,
            }
            .normalized();
            assert!(normalized.h.is_finite() && (0.0..360.0).contains(&normalized.h));
            assert!(normalized.s.is_finite() && (0.0..=1.0).contains(&normalized.s));
            assert!(normalized.v.is_finite() && (0.0..=1.0).contains(&normalized.v));
            assert!(normalized.a.is_finite() && (0.0..=1.0).contains(&normalized.a));
            let rgba = Hsva {
                h: hue,
                s: f32::NAN,
                v: f32::INFINITY,
                a: f32::NEG_INFINITY,
            }
            .to_rgba();
            assert_eq!(rgba.format().len(), 9);
        }
        assert_eq!(hsv_to_rgb(0.0, 2.0, 2.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(0.0, -1.0, 0.5), (128, 128, 128));
        assert_eq!(hsv_to_rgb(0.0, 1.0, -1.0), (0, 0, 0));

        let degenerate = Triangle {
            center: [f32::NAN, f32::INFINITY],
            radius: 0.0,
        };
        assert!(
            degenerate
                .vertices()
                .iter()
                .flatten()
                .all(|value| value.is_finite())
        );
        assert!(
            degenerate
                .vertices()
                .iter()
                .all(|vertex| *vertex == [0.0, 0.0])
        );
        assert!(degenerate.contains([0.0, 0.0]));
        assert!(!degenerate.contains([1.0, 0.0]));
        assert!(!degenerate.contains([f32::NAN, 0.0]));
        assert_valid_weights(
            degenerate.weights([f32::NAN, f32::INFINITY]),
            "退化三角非有限点",
        );

        let huge = Triangle {
            center: [f32::MAX, f32::MIN],
            radius: f32::MAX,
        };
        for vertex in huge.vertices() {
            assert!(vertex.iter().all(|value| value.is_finite()));
        }
        assert_valid_weights(huge.weights([f32::MAX, f32::MIN]), "巨型三角");

        let invalid = Triangle {
            center: [f32::NAN, f32::NEG_INFINITY],
            radius: f32::NAN,
        };
        assert_valid_weights(invalid.weights([f32::NAN, 0.0]), "非有限三角");
        assert_valid_weights(
            invalid.nearest_weights([f32::INFINITY, f32::NEG_INFINITY]),
            "非有限三角最近点",
        );
        assert!(!invalid.contains([f32::NAN, f32::NAN]));
        assert_eq!(
            invalid
                .color(f32::NAN, [f32::NAN, f32::NEG_INFINITY, f32::INFINITY])
                .format()
                .len(),
            9
        );

        let center = [5.0, -7.0];
        assert_eq!(hue_at(center, center), 0.0);
        assert!((0.0..360.0).contains(&hue_at([f32::NAN, 0.0], [0.0, 0.0])));
        assert!((0.0..360.0).contains(&hue_at([0.0, 0.0], [f32::INFINITY, 0.0])));
        let invalid_point = hue_point([f32::NAN, f32::INFINITY], f32::INFINITY, f32::NAN);
        assert!(invalid_point.iter().all(|value| value.is_finite()));
        assert_eq!(hue_point([0.0, 0.0], -5.0, f32::INFINITY), [0.0, 0.0]);
    }

    #[test]
    fn fixed_seed_random_roundtrip_and_color_space_guards() {
        let mut rng = Lcg::new(0x5EED_1234_5678_9ABC);
        for _ in 0..5000 {
            let source = Rgba {
                r: rng.next_byte(),
                g: rng.next_byte(),
                b: rng.next_byte(),
                a: rng.next_byte(),
            };
            let (h, s, v) = rgb_to_hsv(source.r, source.g, source.b);
            assert!((0.0..360.0).contains(&h));
            assert!((0.0..=1.0).contains(&s));
            assert!((0.0..=1.0).contains(&v));
            let roundtrip = hsv_to_rgb(h, s, v);
            assert!(
                roundtrip.0.abs_diff(source.r) <= 1,
                "红色通道往返误差超过 1：{source:?} -> {roundtrip:?}"
            );
            assert!(
                roundtrip.1.abs_diff(source.g) <= 1,
                "绿色通道往返误差超过 1：{source:?} -> {roundtrip:?}"
            );
            assert!(
                roundtrip.2.abs_diff(source.b) <= 1,
                "蓝色通道往返误差超过 1：{source:?} -> {roundtrip:?}"
            );
            let via_hsva = Hsva::from_rgba(source, None, None).to_rgba();
            assert!(via_hsva.r.abs_diff(source.r) <= 1);
            assert!(via_hsva.g.abs_diff(source.g) <= 1);
            assert!(via_hsva.b.abs_diff(source.b) <= 1);
            assert_eq!(via_hsva.a, source.a);
        }

        // 明确不是 HSL：纯红的 V=1（HSL 的 L 会是 0.5）。
        let (hue, saturation, value) = rgb_to_hsv(255, 0, 0);
        assert_eq!(hue, 0.0);
        assert_eq!(saturation, 1.0);
        assert_eq!(value, 1.0);
        let (_, saturation, value) = rgb_to_hsv(128, 64, 64);
        assert_close(value, 128.0 / 255.0, 1.0e-6);
        assert_close(saturation, 0.5, 1.0e-6);

        // 明确不是线性光/双 gamma：50% 明度用编码值 128。
        assert_eq!(hsv_to_rgb(0.0, 0.0, 0.5), (128, 128, 128));
        // 明确不是预乘：A=0 时 RGB 保留。
        let transparent = Rgba {
            r: 200,
            g: 100,
            b: 50,
            a: 0,
        };
        assert_eq!(
            Hsva::from_rgba(transparent, None, None).to_rgba(),
            transparent
        );
    }
}

#[cfg(test)]
mod hsl_tests {
    use super::*;

    #[test]
    fn hsl_reference_colors_and_transparent_rgb() {
        for (h, expected) in [
            (0.0, "#FF000080"),
            (60.0, "#FFFF0080"),
            (120.0, "#00FF0080"),
            (180.0, "#00FFFF80"),
            (240.0, "#0000FF80"),
            (300.0, "#FF00FF80"),
        ] {
            assert_eq!(
                Hsla {
                    h,
                    s: 1.0,
                    l: 0.5,
                    a: 0.5
                }
                .to_rgba()
                .format(),
                expected
            );
        }
        for s in [0.0, 0.3, 1.0] {
            assert_eq!(
                Hsla {
                    h: 33.0,
                    s,
                    l: 1.0,
                    a: 1.0
                }
                .to_rgba(),
                Rgba::WHITE
            );
            assert_eq!(
                Hsla {
                    h: 33.0,
                    s,
                    l: 0.0,
                    a: 1.0
                }
                .to_rgba()
                .format(),
                "#000000FF"
            );
        }
        assert_eq!(
            Hsla {
                h: 0.0,
                s: 1.0,
                l: 0.5,
                a: 0.0
            }
            .to_rgba()
            .format(),
            "#FF000000"
        );
        assert_eq!(
            Hsla {
                h: 0.0,
                s: 0.5,
                l: 0.5,
                a: 1.0
            }
            .to_rgba()
            .format(),
            "#BF4040FF"
        );
    }

    #[test]
    fn hsl_rgba_roundtrip_and_degenerate_color_memory() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let original = Rgba { r, g, b, a: 73 };
                    let actual = Hsla::from_rgba(original, None, None).to_rgba();
                    assert!(
                        actual.r.abs_diff(r) <= 1
                            && actual.g.abs_diff(g) <= 1
                            && actual.b.abs_diff(b) <= 1
                    );
                    assert_eq!(actual.a, 73);
                }
            }
        }
        for text in ["#000000FF", "#FFFFFFFF"] {
            let hsl = Hsla::from_rgba(Rgba::parse(text).unwrap(), Some(210.0), Some(0.7));
            assert_eq!((hsl.h, hsl.s), (210.0, 0.7));
        }
        let gray = Hsla::from_rgba(Rgba::parse("#808080FF").unwrap(), Some(120.0), Some(0.7));
        assert_eq!((gray.h, gray.s), (120.0, 0.0));
        let bounded = Hsla {
            h: f32::NAN,
            s: f32::INFINITY,
            l: f32::NEG_INFINITY,
            a: f32::NAN,
        }
        .to_rgba();
        assert_eq!(bounded.format(), "#000000FF");
    }
}
