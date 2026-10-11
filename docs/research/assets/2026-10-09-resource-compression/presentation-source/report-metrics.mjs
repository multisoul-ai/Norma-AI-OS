export function meetsHalf(samples, sourceBytes, repeats) {
  return samples.length === repeats && samples.every(sample =>
    sample.status === 'ok' && sample.compressed_bytes * 2 <= sourceBytes);
}

export function failureDisplay(samples, timeout) {
  const sample = samples.findLast(item => item.status !== 'ok') ?? {};
  let mark = '未完成';
  if (sample.status === 'unavailable') mark = '工具缺失';
  else if (sample.compress && sample.compress.status !== 'ok') {
    mark = sample.compress.status === 'timeout' ? '压缩超时' : '压缩失败';
  } else if (sample.decompress && sample.decompress.status !== 'ok') {
    mark = sample.decompress.status === 'timeout' ? '解压超时' : '解压失败';
  } else if (sample.status === 'invalid') mark = '还原校验失败';
  const phases = ['compress', 'decompress'].map(key => sample[key]);
  return { mark, times: phases.map(phase => !phase ? '未执行' : phase.status === 'timeout' ? `>${timeout}` : phase.seconds ?? '未知'),
    memory: phases.map(phase => !phase ? '未执行' : phase.max_rss_bytes == null ? '未知' : phase.max_rss_bytes / 2 ** 20) };
}
