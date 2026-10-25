// 运行特效引擎:index.html(风格对比)与 settings.html(设置页)共用。
const $=(t,c,p)=>{const e=document.createElement(t);if(c)e.className=c;if(p)p.append(e);return e};
const clamp=(v,a=0,b=1)=>Math.min(Math.max(v,a),b),ease=u=>1-Math.pow(1-clamp(u),3),frac=x=>x-Math.floor(x);
const hash=n=>frac(Math.sin(n*127.1+311.7)*43758.5453);
const rgba=(c,a)=>`rgba(${c[0]|0},${c[1]|0},${c[2]|0},${clamp(a)})`;
const mix=(a,b,k)=>a.map((v,i)=>v+(b[i]-v)*k);
const ST={ready:{c:[142,142,147],chip:'idle',line:'等待任务…'},thinking:{c:[10,132,255],chip:'thinking',line:'分析调用关系…'},
 tool:{c:[64,200,224],chip:'run_tests',line:'运行工具…'},approval:{c:[255,159,10],chip:'needs approval',line:'等待你的批准'},
 done:{c:[48,209,88],chip:'done',line:'已完成,3 个文件已修改'},failed:{c:[255,69,58],chip:'error',line:'测试失败:2 个用例'}};
const W=460,H=330,X=60,Y=60,CW=340,CH=210;
function rr(c,x,y,w,h,r){c.beginPath();c.roundRect(x,y,w,h,r)}
function inside(c,fn){c.save();rr(c,X,Y,CW,CH,14);c.clip();fn();c.restore()}
// 边界点(矩形周长)+外法线
function perim(u){const P=2*(CW+CH);let s=frac(u)*P;
 if(s<CW)return{x:X+s,y:Y,nx:0,ny:-1};s-=CW;
 if(s<CH)return{x:X+CW,y:Y+s,nx:1,ny:0};s-=CH;
 if(s<CW)return{x:X+CW-s,y:Y+CH,nx:0,ny:1};s-=CW;return{x:X,y:Y+CH-s,nx:-1,ny:0}}

/* ---------- 共用工具 ---------- */
const R0=14,RSX=CW-2*R0,RSY=CH-2*R0,RQ=Math.PI*R0/2,RP=2*RSX+2*RSY+4*RQ,UB=(RSX+RQ+RSY+RQ+RSX/2)/RP;
function perimRR(u){let s=frac(u)*RP;const arc=(cx,cy,a0)=>s2=>{const a=a0+s2/R0;return{x:cx+R0*Math.cos(a),y:cy+R0*Math.sin(a),nx:Math.cos(a),ny:Math.sin(a)}};
 const seg=[[RSX,s2=>({x:X+R0+s2,y:Y,nx:0,ny:-1})],[RQ,arc(X+CW-R0,Y+R0,-Math.PI/2)],[RSY,s2=>({x:X+CW,y:Y+R0+s2,nx:1,ny:0})],[RQ,arc(X+CW-R0,Y+CH-R0,0)],
  [RSX,s2=>({x:X+CW-R0-s2,y:Y+CH,nx:0,ny:1})],[RQ,arc(X+R0,Y+CH-R0,Math.PI/2)],[RSY,s2=>({x:X,y:Y+CH-R0-s2,nx:-1,ny:0})],[RQ,arc(X+R0,Y+R0,Math.PI)]];
 for(const [len,f] of seg){if(s<=len)return f(s);s-=len}return seg[0][1](0)}
const G=(p,m,s)=>Math.exp(-(((p-m)/s)**2));
const beat=(t,p=1200)=>{const m=((t%p)+p)%p;return G(m,0,55)+G(m,p,55)+.65*G(m,230,60)};
const ecg=p=>.12*G(p,.2,.03)-.2*G(p,.385,.012)+G(p,.41,.012)-.38*G(p,.44,.012)+.3*G(p,.63,.045);
function sm(g,k,target,tau=200,init=0){if(g.m[k]===undefined)g.m[k]=init;g.m[k]+=(target-g.m[k])*(1-Math.exp(-g.dt/tau));return g.m[k]}
function glow(c,x,y,r,col,a){if(a<.01)return;const gr=c.createRadialGradient(x,y,0,x,y,r*4);gr.addColorStop(0,rgba(col,a));gr.addColorStop(.3,rgba(col,a*.35));gr.addColorStop(1,rgba(col,0));
 c.fillStyle=gr;c.fillRect(x-r*4,y-r*4,r*8,r*8);c.fillStyle=rgba(mix(col,[255,255,255],.6),Math.min(1,a*1.2));c.beginPath();c.arc(x,y,r*.6,0,7);c.fill()}
function segDist(px,py,a,b){const dx=b[0]-a[0],dy=b[1]-a[1],u=clamp(((px-a[0])*dx+(py-a[1])*dy)/(dx*dx+dy*dy));return Math.hypot(px-a[0]-u*dx,py-a[1]-u*dy)}
/* ---------- 五套风格:draw(c,g) ---------- */
const PAL={ready:[[142,142,147],[120,120,130],[142,142,147],[120,120,130]],
 thinking:[[10,132,255],[175,82,222],[255,100,140],[90,200,250]],tool:[[64,210,230],[10,132,255],[175,82,222],[120,240,220]],
 approval:[[255,159,10],[255,110,90],[255,205,120],[255,135,160]],done:[[48,209,88],[110,230,200],[60,200,120],[180,240,150]],
 failed:[[255,69,58],[190,30,60],[255,100,80],[140,20,40]]};
const palAt=(p,x)=>{const s=frac(x)*4,i=Math.floor(s),f=s-i,e=f*f*(3-2*f);return mix(p[i%4],p[(i+1)%4],e)};
const MODES={ready:[[1,2]],thinking:[[2,3],[3,4],[1,4],[3,5]],tool:[[5,6],[4,7]],approval:[[1,3]],done:[[2,5]],failed:[[6,7]]};
const STYLES=[
{key:'classic',name:'经典 Classic',desc:'现有表现:思考与调用工具时一条光蛇沿边框游走,等你时边框暖色呼吸,完成时冷色呼吸与外缘柔光保留到查看,失败无边框特效。',tags:['现状','边框'],
 draw(c,g){const n=g.name,t=g.t,col=g.rgb;
  const ring=(lw,a)=>{rr(c,X-.5,Y-.5,CW+1,CH+1,14.5);c.strokeStyle=rgba(col,a);c.lineWidth=lw;c.stroke()};
  if(n==='thinking'||n==='tool'){g.m.ph=(g.m.ph||0)+g.dt*(n==='tool'?.0005:.00028);const N=60,L=.16;c.lineCap='round';c.lineWidth=2.2;
   for(let i=0;i<N;i++){const u=g.m.ph-L*i/N,p=perimRR(u),q=perimRR(u-L/N);c.strokeStyle=rgba(col,.95*(1-i/N));c.beginPath();c.moveTo(p.x,p.y);c.lineTo(q.x,q.y);c.stroke()}
   const h=perimRR(g.m.ph);glow(c,h.x,h.y,2,col,.6);c.lineCap='butt'}
  // 与 motion/breathing.rs 一致:等你 = 暖色 1.2s 呼吸;完成未查看 = 冷色 2.5s 色相循环 + 外缘柔光;失败无边框特效
  if(n==='approval'){const b=.5-.5*Math.cos(t/1200*6.283);ring(6,.08+.12*b);ring(1.5,.35+.4*b)}
  if(n==='done'&&!g.viewed){const p=t/2500,cool=palAt([[90,200,250],[175,82,222],[10,132,255],[90,200,250]],p),b=.5+.5*Math.sin(p*6.283);
   rr(c,X-.5,Y-.5,CW+1,CH+1,14.5);[[9,.06],[5,.12]].forEach(([lw,a])=>{c.strokeStyle=rgba(cool,a*(.75+.25*b));c.lineWidth=lw;c.stroke()});c.strokeStyle=rgba(cool,.4+.6*b);c.lineWidth=1.5;c.stroke()}}},
{key:'dot',minimal:true,ownDot:true,name:'极简 · 一个点 Dot',desc:'只动标题栏的状态点,卡片其余部分完全静止。思考时慢呼吸;每次切换工具外扩一圈细环;等你时按心跳双搏;完成时绿点外常驻一个细环,点击卡片后消失;失败时红点静止。',tags:['极简','零干扰','推荐默认'],
 draw(c,g){const n=g.name,t=g.t,col=g.rgb,x=X+18.6,y=Y+18.9,rm=g.rm;let r=5,halo=0;
  if(!rm&&n==='thinking'){const b=.5-.5*Math.cos(t/2000*6.283);r=5+1.2*b;halo=.35*b}
  if(!rm&&n==='approval'){const b=beat(t);r=5+1.6*b;halo=.5*b}
  if(halo>.01)glow(c,x,y,r*.9,col,halo);
  c.fillStyle=rgba(col,1);c.beginPath();c.arc(x,y,r,0,7);c.fill();
  if(n==='tool'&&!rm&&g.ping<420){const u=ease(g.ping/420);c.strokeStyle=rgba(col,.8*(1-u));c.lineWidth=1.2;c.beginPath();c.arc(x,y,5+10*u,0,7);c.stroke()}
  if(n==='done'&&!g.viewed){c.strokeStyle=rgba(col,.7);c.lineWidth=1.2;c.beginPath();c.arc(x,y,8.5,0,7);c.stroke()}}},

{key:'hairline',minimal:true,name:'极简 · 一条线 Hairline',desc:'只用卡片的 1px 边框。思考时一段颜色沿边框缓慢流过;调用工具时流速加快;等你时整圈琥珀色随心跳亮暗;完成时绿线从状态点出发绕一圈后停住;失败时整圈变红。',tags:['极简','边框','安静'],
 draw(c,g){const n=g.name,t=g.t,el=g.el,col=g.rgb,rm=g.rm,N=160,U0=.015;
  g.m.ph=(g.m.ph||0)+g.dt*(n==='tool'?.00028:.00012);
  const segA=u=>{if(n==='ready')return 0;if(n==='failed')return .85;
   if(n==='approval')return rm?.8:.35+.6*beat(t);
   if(n==='done'){if(g.viewed)return 0;const p=rm?1:ease(el/700);return frac(u-U0)<=p?.85:0}
   if(rm)return .6;let d=frac(u-g.m.ph);return d<.28?Math.sin(d/.28*Math.PI)*.95:0};
  c.lineWidth=1.2;
  const full=n==='failed'||n==='approval'||(n==='done'&&!g.viewed&&(rm||el>=700))||(rm&&(n==='thinking'||n==='tool'));
  if(full){rr(c,X+.6,Y+.6,CW-1.2,CH-1.2,13.6);c.strokeStyle=rgba(col,segA(.5));c.stroke();return}
  for(let i=0;i<N;i++){const u=i/N,a=segA(u+.5/N);if(a<.01)continue;const p=perimRR(u),q=perimRR(u+1/N);
   c.strokeStyle=rgba(col,a);c.beginPath();c.moveTo(p.x,p.y);c.lineTo(q.x,q.y);c.stroke()}}},

{key:'underline',minimal:true,name:'极简 · 一道光 Underline',desc:'只用标题栏下方的分隔线。思考时一小段光在线上来回游走;调用工具时光段滑到工具名下方停住,每次切换轻轻一闪;等你时整条线变琥珀色呼吸;完成时绿色从左向右铺满并保持;失败时整条线变红。',tags:['极简','分隔线','像进度条'],
 draw(c,g){const n=g.name,t=g.t,el=g.el,col=g.rgb,rm=g.rm,y=Y+CH*.18,x0=X+1,x1=X+CW-1,L=x1-x0;
  const line=(a,b,al,lw=1.5)=>{c.strokeStyle=rgba(col,al);c.lineWidth=lw;c.beginPath();c.moveTo(a,y);c.lineTo(b,y);c.stroke()};
  const seg=(cx,w,al)=>{const gr=c.createLinearGradient(cx-w/2,0,cx+w/2,0);gr.addColorStop(0,rgba(col,0));gr.addColorStop(.5,rgba(col,al));gr.addColorStop(1,rgba(col,0));c.strokeStyle=gr;c.lineWidth=1.6;c.beginPath();c.moveTo(cx-w/2,y);c.lineTo(cx+w/2,y);c.stroke()};
  if(n==='thinking'){if(rm)line(x0,x1,.5);else seg(x0+L*(.5-.5*Math.cos(t/2400*6.283)),70,.95)}
  if(n==='tool'){const tx=sm(g,'tx',x1-40,rm?1:260,x0+L/2);seg(tx,rm?60:60+30*Math.max(0,1-g.ping/400),rm?.9:.7+.3*Math.max(0,1-g.ping/400))}
  if(n==='approval')line(x0,x1,rm?.8:.3+.6*beat(t));
  if(n==='done'&&!g.viewed)line(x0,x0+L*(rm?1:ease(el/600)),.85);
  if(n==='failed')line(x0,x1,.85)}},
{key:'aura',name:'灵光 Aura',desc:'卡片边缘透出一圈光谱色的柔光,像光穿过玻璃边缘发生色散,并渗进卡片内部。思考时光谱缓慢流转、明暗起伏;工具调用时一束亮光沿边缘疾行;等你时转暖色并随心跳呼吸;完成时一道亮光绕一圈后沉淀为薄荷绿;失败时光谱收缩成一圈暗红。',tags:['边缘光','色散','克制'],
 draw(c,g){const n=g.name,t=g.t,el=g.el,a=1-Math.exp(-g.dt/220),N=144;
  g.m.pal=(g.m.pal||PAL.ready.map(x=>x.slice())).map((v,i)=>mix(v,PAL[n==='done'&&g.viewed?'ready':n][i],a));
  g.m.rot=(g.m.rot||0)+g.dt*{ready:0,thinking:.00007,tool:.00018,approval:.00003,done:.00004,failed:0}[n];
  let I=sm(g,'I',{ready:0,thinking:.8,tool:1,approval:.75,done:g.viewed?0:el<700?1.2:.55,failed:el<260?1.1:.4}[n],200);
  if(n==='approval')I*=.55+.45*beat(t);
  if(n==='failed'&&el<700&&hash(Math.floor(el/60))>.6)I*=.3;
  const wid=sm(g,'wd',n==='failed'?.45:1,300,1);if(I<.01)return;
  const U0=(RSX-24)/RP,pts=[];
  for(let i=0;i<=N;i++){const u=i/N,p=perimRR(u);let k=.55+.45*Math.sin(u*6.283*3+t*.0011)*Math.sin(u*6.283*2-t*.0007);
   if(n==='tool'){let du=frac(u-U0-g.ping/650);du=Math.min(du,1-du);k+=1.6*G(du,0,.035)*Math.max(0,1-g.ping/900)}
   if(n==='done'&&el<900){let du=frac(u-el/900);du=Math.min(du,1-du);k+=1.4*G(du,0,.05)}
   pts.push({x:p.x+p.nx,y:p.y+p.ny,col:palAt(g.m.pal,u+g.m.rot),k})}
  // 低分辨率离屏画布放大即得柔光,不用逐帧 blur 滤镜
  const mk=sc=>{const o=document.createElement('canvas');o.width=Math.ceil(W*sc);o.height=Math.ceil(H*sc);return o};
  if(!g.m.o1){g.m.o1=mk(.25);g.m.o2=mk(.1)}
  const strokeAll=(ctx,lw,al)=>{for(let i=0;i<N;i++){const p=pts[i],q=pts[i+1];ctx.strokeStyle=rgba(p.col,al*I*p.k);ctx.lineWidth=lw;ctx.beginPath();ctx.moveTo(p.x,p.y);ctx.lineTo(q.x,q.y);ctx.stroke()}};
  [[g.m.o1,.25,6],[g.m.o2,.1,9]].forEach(([o,sc,lw])=>{const x=o.getContext('2d');x.setTransform(sc,0,0,sc,0,0);x.clearRect(0,0,W,H);x.lineCap='round';strokeAll(x,lw*wid,1)});
  c.save();c.imageSmoothingEnabled=true;c.imageSmoothingQuality='high';c.globalAlpha=.75;c.drawImage(g.m.o2,0,0,W,H);c.globalAlpha=.7;c.drawImage(g.m.o1,0,0,W,H);c.restore();
  inside(c,()=>{c.globalAlpha=.4;c.drawImage(g.m.o2,0,0,W,H);c.globalAlpha=1});
  c.lineCap='round';strokeAll(c,1.3,1);c.lineCap='butt'}},

{key:'contour',name:'等高线 Contour',desc:'卡片底面是一张会呼吸的地形图。思考时几座山丘缓慢漂移、等高线随之流动,最高点有一枚峰顶标记;调用工具时工具芯片下隆起一座尖峰;等你时从中心荡开同心波纹,随心跳起伏;完成时地形收拢成一座平静的圆丘;失败时地表塌陷成深坑。',tags:['内部线框','数据地形','平静'],
 draw(c,g){const n=g.name,t=g.t,el=g.el,col=g.rgb,S=8,cols=Math.ceil(CW/S)+1,rows=Math.ceil(CH/S)+1;
  const wt=sm(g,'wt',n==='thinking'||n==='tool'?1:n==='ready'||(n==='done'&&g.viewed)?.25:0,380,.25),wo=sm(g,'wo',n==='tool'?1:0,260),
   wa=sm(g,'wa',n==='approval'?1:0,380),wd=sm(g,'wd',n==='done'&&!g.viewed?1:0,480),wf=sm(g,'wf',n==='failed'?1:0,320);
  g.m.tc=(g.m.tc||0)+g.dt*(n==='tool'?1.8:n==='ready'?.3:1);const T=g.m.tc,b=beat(t),pk=Math.max(0,1-g.ping/500),ccx=X+CW/2,ccy=Y+CH/2+12;
  const F=new Float32Array(cols*rows);let best=-1e9,bx=0,by=0;
  for(let j=0;j<rows;j++)for(let i=0;i<cols;i++){const x=X+i*S,y=Y+j*S,r=Math.hypot(x-ccx,y-ccy);let v=.16*Math.sin(x*.018+T*.0003)*Math.cos(y*.025-T*.00025);
   if(wt>.001)for(let k=0;k<3;k++){const hx=X+CW*(.5+.38*Math.sin(T*.00031*(k+1)+k*2)),hy=Y+CH*(.52+.3*Math.cos(T*.00023*(k+1.3)+k));v+=wt*Math.exp(-((x-hx)**2+(y-hy)**2)/6050)*(k?.8:1.1)}
   if(wo>.001)v+=wo*Math.exp(-((x-X-CW+40)**2+(y-Y-34)**2)/2312)*(1.1+.7*pk);
   if(wa>.001)v+=wa*(Math.cos(r*.075-t*.0035)*.55*Math.exp(-r/170)*(.55+.45*b)+.35*Math.exp(-r*r/8000)*b);
   if(wd>.001)v+=wd*1.3*Math.exp(-r*r/14450)*(1+.05*Math.sin(t*.0015));
   if(wf>.001)v+=wf*(-1.1*Math.exp(-r*r/9800)+(hash(i*31+j*17+Math.floor(el/70))-.5)*1.3*Math.exp(-el/220)*(el<700?1:0));
   F[j*cols+i]=v;if(v>best&&j>4){best=v;bx=x;by=y}}
  const lv=[-.8,-.55,-.3,-.08,.14,.32,.5,.68,.86,1.04,1.22],vis=.35+.65*clamp(g.lvl+.3);
  inside(c,()=>{c.lineWidth=1;lv.forEach((L,li)=>{const k=(li+1)/lv.length;c.strokeStyle=rgba(col,(.14+.62*k)*vis);c.beginPath();
   for(let j=0;j<rows-1;j++)for(let i=0;i<cols-1;i++){const a=F[j*cols+i],bb=F[j*cols+i+1],d=F[(j+1)*cols+i],e=F[(j+1)*cols+i+1],x=X+i*S,y=Y+j*S,p=[];
    if((a-L)*(bb-L)<0)p.push([x+S*(L-a)/(bb-a),y]);if((bb-L)*(e-L)<0)p.push([x+S,y+S*(L-bb)/(e-bb)]);
    if((d-L)*(e-L)<0)p.push([x+S*(L-d)/(e-d),y+S]);if((a-L)*(d-L)<0)p.push([x,y+S*(L-a)/(d-a)]);
    if(p.length>=2){c.moveTo(p[0][0],p[0][1]);c.lineTo(p[1][0],p[1][1])}if(p.length===4){c.moveTo(p[2][0],p[2][1]);c.lineTo(p[3][0],p[3][1])}}
   c.stroke()});
   const mk=n==='thinking'||n==='tool'||(n==='done'&&!g.viewed);const ma=sm(g,'mk',mk?1:0,200);
   if(ma>.02){const px=sm(g,'px',bx,120,bx),py=sm(g,'py',by,120,by);c.strokeStyle=rgba(col,.9*ma);c.lineWidth=1;
    c.beginPath();c.moveTo(px-5,py);c.lineTo(px+5,py);c.moveTo(px,py-5);c.lineTo(px,py+5);c.stroke();c.beginPath();c.arc(px,py,8+2*Math.sin(t*.004),0,7);c.stroke();glow(c,px,py,1.4,col,.8*ma)}})}},
{key:'ferro',name:'磁流体 Ferrofluid',desc:'一层液态金属包裹着卡片,像被磁场牵动的活体。思考时表面蠕动起伏;调用工具时朝工具芯片竖起尖刺;等你时汇聚成一颗跳动的液滴;完成时尖刺退去、镜面光掠过;失败时整体下垂滴落。',tags:['外缘膜','液态金属','活体'],
 draw(c,g){const n=g.name,t=g.t,el=g.el,N=240;
  const amp=sm(g,'amp',{ready:.8,thinking:4.6,tool:4.2,approval:1.6,done:2.4,failed:1.6}[n],260,.8);
  const spike=sm(g,'spike',{ready:0,thinking:9,tool:18,approval:0,done:0,failed:0}[n],200);
  const drop=sm(g,'drop',n==='approval'?1:0,320),sag=sm(g,'sag',n==='failed'?1:0,450);
  const pts=[];
  for(let i=0;i<N;i++){const u=i/N,p=perimRR(u);
   let d=2.4+amp*(.6+.4*Math.sin(u*6.283*4-t*.0016))+amp*.5*Math.sin(u*6.283*11+t*.0023);
   let s=0;for(let k=0;k<16;k++){let du=Math.abs(u-hash(k+3));du=Math.min(du,1-du);let h=.35+.65*(.5+.5*Math.sin(t*.004*(1+hash(k))+k*1.7));
    if(n==='tool')h*=(p.ny<-.5?1.2:.25)*(1+.9*Math.max(0,1-g.ping/280));s+=G(du,0,.0055)*h}
   d+=spike*s;
   let db=Math.abs(u-UB);db=Math.min(db,1-db);d=d*(1-.55*drop)+drop*(12+9*beat(t))*G(db,0,.045);
   if(sag>0){if(p.ny>.5)for(let k=0;k<3;k++){const uk=UB+(k-1)*.09+(hash(k)-.5)*.03;d+=sag*Math.min(34,el*.035*(.5+hash(k+8)))*G(u,uk,.007)+sag*3*G(u,uk,.03)}if(p.ny<0)d*=1-.65*sag}
   pts.push([p.x+p.nx*d,p.y+p.ny*d])}
  const path=()=>{c.beginPath();pts.forEach((q,i)=>i?c.lineTo(q[0],q[1]):c.moveTo(q[0],q[1]));c.closePath()};
  const body=mix(g.rgb,[6,6,12],.8);
  path();c.roundRect(X,Y,CW,CH,14);
  const gr=c.createLinearGradient(0,Y-25,0,Y+CH+40);gr.addColorStop(0,rgba(mix(body,[255,255,255],.16),.96));gr.addColorStop(.45,rgba(body,.96));gr.addColorStop(1,rgba(mix(body,[0,0,0],.5),.96));
  c.fillStyle=gr;c.fill('evenodd');
  path();c.strokeStyle=rgba(g.rgb,.08+.12*g.lvl);c.lineWidth=6;c.stroke();c.strokeStyle=rgba(g.rgb,.3+.55*g.lvl);c.lineWidth=1.1;c.stroke();
  const hl=(pos,len,a)=>{c.lineWidth=1.6;for(let i=0;i<N;i++){let du=Math.abs(i/N-pos);du=Math.min(du,1-du);if(du>len)continue;const j=(i+1)%N;
   c.strokeStyle=rgba([255,255,255],a*(1-du/len));c.beginPath();c.moveTo(pts[i][0],pts[i][1]);c.lineTo(pts[j][0],pts[j][1]);c.stroke()}};
  hl(frac(t*.00012),.07,.45);
  if(n==='done'&&el<1000)hl(frac(ease(el/1000)*1.0+.05),.12,.95*(1-el/1000));
  if(n==='done'&&!g.viewed){c.strokeStyle=rgba(g.rgb,.25+.15*Math.sin(t*.002));c.lineWidth=2;path();c.stroke()}}},

{key:'cymatics',name:'声振 Cymatics',desc:'卡片里铺着一层细沙,像放在振动板上。思考时振动频率不断变换,沙粒反复聚成不同的克拉尼图案;调用工具时切到高频,图案更细密;等你时每一下心跳都让沙粒轻颤;完成时振动停止,沙粒落定成一幅清晰对称的图案;失败时剧烈震散后坍落到底部。',tags:['内部粒子','物理图案','可涌现'],
 draw(c,g){const n0=g.name,n=n0==='done'&&g.viewed?'ready':n0,t=g.t,el=g.el,dt=g.dt,col=g.rgb,x0=X+8,y0=Y+42,w=CW-16,h=CH-50,N=700;
  if(!g.m.p){g.m.p=[];for(let i=0;i<N;i++)g.m.p.push([x0+Math.random()*w,y0+Math.random()*h,0])}
  const ms=MODES[n],[nn,mm]=ms[(n==='thinking'?Math.floor(t/2600):n==='tool'?Math.floor(t/1800):0)%ms.length];
  let vib={ready:.15,thinking:.5,tool:1,approval:.2+1.3*beat(t),done:el<300?.6:.02,failed:el<500?3:0}[n];if(n==='tool')vib+=2*Math.max(0,1-g.ping/300);
  const K=n==='failed'?0:2.4,grav=n==='failed'&&el>350?.0012:0,PI=Math.PI;
  const f=(u,v)=>Math.cos(nn*PI*u)*Math.cos(mm*PI*v)-Math.cos(mm*PI*u)*Math.cos(nn*PI*v);
  inside(c,()=>{g.m.p.forEach(p=>{const u=(p[0]-x0)/w,v=(p[1]-y0)/h,e=.002,F0=f(u,v),fu=(f(u+e,v)-f(u-e,v))/(2*e)/w,fv=(f(u,v+e)-f(u,v-e))/(2*e)/h;
    p[0]-=F0*fu*K*dt;p[1]-=F0*fv*K*dt;const j=vib*Math.abs(F0)*.06*dt+(n==='failed'&&el<500?vib*.4:0);p[0]+=(Math.random()-.5)*j;p[1]+=(Math.random()-.5)*j;
    if(grav){p[2]+=grav*dt;p[1]+=p[2]*dt}else p[2]=0;
    p[0]=clamp(p[0],x0,x0+w);if(p[1]>y0+h){p[1]=y0+h-Math.random()*3;p[2]=0}p[1]=Math.max(p[1],y0);
    const st=1-Math.min(1,Math.abs(F0)*4),txt=(p[1]<Y+108&&p[0]<X+CW*.72)?.4:1,al=(.25+.7*st)*txt*(.5+.5*clamp(g.lvl+.2));
    c.fillStyle=rgba(st>.85?mix(col,[255,255,255],.3):col,al);c.fillRect(p[0]-.85,p[1]-.85,1.7,1.7)})})}},
{key:'scanner',name:'扫描 Scanner',desc:'终端美学。一道扫描线巡视卡片内部;审批时只剩闪烁的光标;失败是画面撕裂。',tags:['终端感','内部扫描','工程味'],
 draw(c,g){const top=Y+38,bot=Y+CH;g.ph+=g.dt*(g.name==='tool'?1/900:g.name==='thinking'?1/2000:0);
  inside(c,()=>{
   if(g.lvl>0.02&&g.name!=='failed'){c.fillStyle=rgba(g.rgb,.05*g.lvl);for(let y=top;y<bot;y+=6)c.fillRect(X,y,CW,1)}
   let sy=null,al=1;
   if(g.name==='thinking'||g.name==='tool')sy=top+frac(g.ph)*(bot-top);
   if(g.name==='done'&&g.el<600){sy=top+ease(g.el/600)*(bot-top);al=1-g.el/900}
   if(sy!==null){const gr=c.createLinearGradient(0,sy-70,0,sy);gr.addColorStop(0,rgba(g.rgb,0));gr.addColorStop(1,rgba(g.rgb,.32*g.lvl*al));c.fillStyle=gr;c.fillRect(X,Math.max(top,sy-70),CW,Math.min(70,sy-top));
    c.fillStyle=rgba(mix(g.rgb,[255,255,255],.35),.95*al);c.fillRect(X,sy-1,CW,1.5)}
   if(g.name==='approval'){if(Math.floor(g.t/530)%2===0){c.fillStyle=rgba(g.rgb,.95);c.fillRect(X+16,Y+CH-36,9,16)}
    c.fillStyle=rgba(g.rgb,.35);for(let x=X;x<X+CW;x+=10)c.fillRect(x,Y+CH-2,5,2)}
   if(g.name==='tool'&&g.ping<160){c.fillStyle=rgba(g.rgb,.28);for(let k=0;k<2;k++){c.fillRect(X+(hash(k+g.ping)*20-10),top+hash(k*5+3)*(bot-top),CW,3+hash(k)*4)}}
   if(g.name==='failed'){if(g.el<420){const f=Math.floor(g.el/60);for(let k=0;k<6;k++){c.fillStyle=rgba(g.rgb,.28);c.fillRect(X+(hash(k+f*9)-.5)*26,top+hash(k*3+f)*(bot-top),CW,3+hash(k+f*2)*8)}}
    c.fillStyle=rgba(g.rgb,.35);c.fillRect(X,top+(bot-top)*.35,CW,1);c.fillRect(X,top+(bot-top)*.7,CW,1)}
  })}}
];

/* ---------- 卡片实例:每组风格 × 六状态 ---------- */
const cards=[];const TOOLS=['read_file','edit_file','run_tests','grep'];
const STATES=[['ready','准备'],['thinking','思考'],['tool','工具调用'],['approval','等待审批'],['done','完成(未查看)'],['failed','失败']];
function makeCard(style,box,state,label){
 const cell=$('div','cell',box);
 const st=$('div','stage',cell),cv=$('canvas','',st),dpr=Math.min(devicePixelRatio||1,2);cv.width=W*dpr;cv.height=H*dpr;
 const card=$('div','card',st);card.innerHTML='<div class="bar"><span class="dot"></span><span>重构登录模块</span><span class="chip"></span></div><div class="body"><b>›</b> claude code · auth/session.rs<br>读取 3 个文件,准备修改<br><span class="ln"></span></div>';
 $('div','lab',cell).textContent=label;
 const now=performance.now(),dot=card.querySelector('.dot');
 const g={m:{},style,c:cv.getContext('2d'),dpr,name:state,t0:now,lvl:0,rgb:ST.ready.c.slice(),k:0,ph:0,dt:16,ping:1e9,pingT:-1e9,last:now,viewed:state!=='done',tool:2,stage:st,paused:false};
 g.paint=()=>{const s=ST[g.name];dot.style.background=`rgb(${s.c})`;dot.style.visibility=g.style.ownDot?'hidden':'';
  card.querySelector('.chip').textContent=g.name==='tool'?TOOLS[g.tool]:s.chip;
  card.querySelector('.ln').textContent=g.name==='tool'?'运行 '+TOOLS[g.tool]:s.line};
 // 切状态不重置 lvl/rgb:颜色与强度从当前值平滑过去(可中断)
 g.set=n=>{if(n===g.name)return;const t=performance.now();g.name=n;g.t0=t;g.viewed=n!=='done';if(n==='tool')g.pingT=t;g.paint()};
 g.setStyle=s=>{if(s===g.style)return;g.style=s;g.m={};g.paint()};
 g.replay=()=>{const n=performance.now();g.t0=n;g.pingT=g.name==='tool'?n:-1e9;g.lvl=0;g.rgb=ST.ready.c.slice();g.k=0;g.ph=0;g.m={}};
 card.onclick=()=>{g.viewed=true;if(g.name==='done')g.t0=performance.now()};
 g.paint();cards.push(g);return g}
// 页面可覆盖:是否减少动态效果
let reduce=()=>false;
function frame(now){
 cards.forEach(g=>{if(g.paused){g.last=now;return}g.dt=Math.min(now-g.last,50);g.last=now;g.el=now-g.t0;g.t=now;
  if(g.name==='tool'&&now-g.pingT>1800){g.pingT=now;g.tool=(g.tool+1)%TOOLS.length;g.paint()}g.ping=now-g.pingT;
  const tgtL={ready:.0,thinking:1,tool:1.2,approval:1,done:g.viewed?0:.5,failed:g.el>900&&true?.6:1}[g.name];
  const a=1-Math.exp(-g.dt/140);g.lvl+=(tgtL-g.lvl)*a;g.rgb=mix(g.rgb,ST[g.name].c,a);g.k+=(((g.name==='thinking'||g.name==='tool')?1:0)-g.k)*a;
  const c=g.c;c.setTransform(g.dpr,0,0,g.dpr,0,0);c.clearRect(0,0,W,H);
  g.rm=reduce();if(g.rm&&!g.style.minimal){const lv=g.name==='ready'?0:g.name==='done'&&g.viewed?0:1;c.save();rr(c,X,Y,CW,CH,14);c.clip();c.fillStyle=rgba(g.rgb,.8*clamp(g.el/160)*lv);c.fillRect(X,Y+CH-3,CW,3);c.restore()}
  else g.style.draw(c,g)});
 requestAnimationFrame(frame)}
