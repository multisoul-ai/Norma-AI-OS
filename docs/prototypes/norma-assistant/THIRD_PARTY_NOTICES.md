# 原型第三方来源

当前仅「极光」使用 React Bits Orb 改编源码；「玻璃」与「水银」为本轮原创光学 shader。下面 Plasma、Iridescence、Prism 和 Silk 为旧版本来源记录，其实现已从当前原型移除。设计网站仅提供视觉参考，未复制其美术或视频。

极光方案与 Simplex noise 参考并改编自 [React Bits Orb](https://github.com/DavidHDev/react-bits/blob/main/src/content/Backgrounds/Orb/Orb.jsx)，作为 Norma 助手应用的一部分。已适配独立 WebGL、连续状态时钟、减少动效和透明背景。

流光方案直接移植自 [React Bits Plasma](https://github.com/DavidHDev/react-bits/blob/main/src/content/Backgrounds/Plasma/Plasma.jsx) 的体积光线步进 shader，保留旋转场、距离估计与光强累积；没有使用 React/OGL 组件。适配 WebGL1 的双曲正切等价式，显式初始化步进深度和累积器，使用 48 次步进（步长补偿 60/48=1.25），调整冷色调和局部软边，并接入真实声音的连续包络。星云和波纹实现已移除。

新增三种方案同样直接改编自 React Bits：

- [Iridescence](https://github.com/DavidHDev/react-bits/blob/main/src/content/Backgrounds/Iridescence/Iridescence.jsx)：保留 8 层三角函数干涉场，映射到局部球面，增加珍珠反射、软边和声音反馈。
- [Prism](https://github.com/DavidHDev/react-bits/blob/main/src/content/Backgrounds/Prism/Prism.jsx)：保留上半八面体距离场、100 步光线累积及 tanh 色调映射，适配连续旋转与冷色体积光，关闭源随机噪点。
- [Silk](https://github.com/DavidHDev/react-bits/blob/main/src/content/Backgrounds/Silk/Silk.jsx)：保留连续褶皱公式，移除像素颗粒，映射为银青球面织光；未引入 React Three Fiber 或 Three.js。

源码读取于 2026-10-04。极光作为当前助手原型的一部分使用，其他四种仅为历史版本记录，完整许可也嵌入 `index.html`，可离线运行。

React Bits 组件适用以下完整许可：

MIT + Commons Clause License Condition v1.0

Copyright (c) 2026 David Haz

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, and distribute the Software **as part of an application, website, or product**, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

## Commons Clause Restriction

You may use this Software, including for any commercial purpose, **so long as you do not sell, sublicense, or redistribute the components themselves-whether alone, in a bundle, or as a ported version.**

## No Warranty

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
