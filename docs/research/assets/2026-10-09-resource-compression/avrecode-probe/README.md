# avrecode 功能适配探测

此目录保存失败证据，不参与通用算法耗时排名。`probe.json` 中三个原 MP4 均在压缩阶段退出 -6，未形成可验证的压缩或还原结果，压缩率未知。`*-compress.log` 的 AddressSanitizer 记录一致：`decode_cabac_residual_dc_internal`、`h264_cabac.c:1789`、读取零页地址 `0x40`。

本次源码为 avrecode `0749548cc337c59933da84063c1edc97ac11d259`，其 `ffmpeg` 子模块来自 `https://github.com/dropbox/libavcodec-hooks.git`，revision `48e5c63d9de6575b17587fcf3e83e844e9fe4a25`。本机 protobuf 为 36.2。`build-compatibility.patch` 仅将 Makefile 的 C++14 改为 C++17、libstdc++ 改为 libc++，保留 AddressSanitizer；原编解码逻辑未修改。

已从实际生成的 `ffmpeg/config.h` 复核本次配置：

```text
--disable-programs --disable-doc --disable-network --disable-asm
--disable-everything --enable-decoder=h264 --enable-demuxer=mov
--enable-protocol=file
```

可从 Norma 仓库根目录，在独立临时目录复现构建；先保证视频字节与 `probe.json` 相同。需要可用的 C/C++ 编译器、make、pkg-config、protobuf 及 protoc。复现建议使用单个构建任务，构建并行度不用于本次功能判定。

```bash
evidence_dir="$PWD/docs/research/assets/2026-10-09-resource-compression/avrecode-probe"
input_file="$PWD/crates/app/assets/feature-guide/demo-layout-command-m.mp4"
probe_root=$(mktemp -d)
git clone https://github.com/dropbox/avrecode.git "$probe_root/avrecode"
git -C "$probe_root/avrecode" checkout --detach 0749548cc337c59933da84063c1edc97ac11d259
git -C "$probe_root/avrecode" submodule update --init --checkout ffmpeg
git -C "$probe_root/avrecode/ffmpeg" checkout --detach 48e5c63d9de6575b17587fcf3e83e844e9fe4a25
git -C "$probe_root/avrecode" apply "$evidence_dir/build-compatibility.patch"
cd "$probe_root/avrecode/ffmpeg"
./configure --disable-programs --disable-doc --disable-network --disable-asm \
  --disable-everything --enable-decoder=h264 --enable-demuxer=mov --enable-protocol=file
make -j1
cd "$probe_root/avrecode"
make -j1
./recode compress "$input_file" "$probe_root/output.avrec"
```

`configure.log`、`ffmpeg-build.log`、`recode-build-original.log` 和 `recode-build-cxx17.log` 保留真实构建输出；最后一份记录编译和链接参数。部分 configure 自动检测到的系统库可能随环境变化，因此不能把本机崩溃推断为所有平台或构建均不可用。当前全部视频的 H.264 profile 与 CABAC 盘点见 `h264-profiles.json`。
