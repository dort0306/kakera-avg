cbuffer Params : register(b0) {float4 multiplyColor;float4 screenColor;float4 mode;float4 effect;float4 dimensions;};
Texture2D sourceImage : register(t0);
Texture2D maskImage : register(t1);
SamplerState imageSampler : register(s0);
struct Vertex {float2 pos:POSITION;float2 uv:TEXCOORD0;float4 color:COLOR0;};
struct Pixel {float4 pos:SV_POSITION;float2 uv:TEXCOORD0;float4 color:COLOR0;};
Pixel vs_main(Vertex v){Pixel o;o.pos=float4(v.pos,0,1);o.uv=v.uv;o.color=v.color;return o;}
float4 ps_ui(Pixel p):SV_TARGET {return p.color*sourceImage.Sample(imageSampler,p.uv);}
float4 ps_model(Pixel p):SV_TARGET {
 float4 c=sourceImage.Sample(imageSampler,float2(p.uv.x,1-p.uv.y));
 if(mode.z>0.5) return c.aaaa; // A hidden source still contributes texture alpha to its clipping mask.
 float coverage=1;
 if(mode.y>0.5){coverage=maskImage.Sample(imageSampler,p.pos.xy/dimensions.xy).a;if(mode.y>1.5)coverage=1-coverage;}
 float a=saturate(c.a*mode.x*coverage);
 float3 rgb=saturate(c.rgb*multiplyColor.rgb+screenColor.rgb*(1-c.rgb*multiplyColor.rgb));
 return float4(rgb*a,a);
}
float4 ps_filter(Pixel p):SV_TARGET {
 float2 uv=p.uv;
 if(effect.x>3.5){float2 small=ceil(dimensions.xy/effect.y);uv=(floor(uv*small)+0.5)/small;}
 float4 c=sourceImage.Sample(imageSampler,uv);
 if(effect.x<1.5){float l=dot(c.rgb,float3(.299,.587,.114));c.rgb=l.xxx;}
 else if(effect.x<2.5){c.rgb=saturate(float3(dot(c.rgb,float3(.393,.769,.189)),dot(c.rgb,float3(.349,.686,.168)),dot(c.rgb,float3(.272,.534,.131))));}
 else if(effect.x<3.5){
   float4 sum=0;float weights=0;int extent=(int)ceil(effect.y*3);float2 step=effect.z<.5?float2(1/dimensions.x,0):float2(0,1/dimensions.y);
   [loop] for(int i=-extent;i<=extent;i++){float w=exp(-float(i*i)/(2*effect.y*effect.y));float4 s=sourceImage.Sample(imageSampler,uv+step*i);if(effect.z<.5)s.rgb*=s.a;sum+=s*w;weights+=w;}
   return sum/weights;
 }
 return float4(c.rgb*c.a,c.a);
}
