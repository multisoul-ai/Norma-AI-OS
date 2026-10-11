def failure_display(samples, timeout):
    sample = next((item for item in reversed(samples) if item['status'] != 'ok'), {})
    mark = '未完成'
    if sample.get('status') == 'unavailable':
        mark = '工具缺失'
    elif sample.get('compress') and sample['compress']['status'] != 'ok':
        mark = '压缩超时' if sample['compress']['status'] == 'timeout' else '压缩失败'
    elif sample.get('decompress') and sample['decompress']['status'] != 'ok':
        mark = '解压超时' if sample['decompress']['status'] == 'timeout' else '解压失败'
    elif sample.get('status') == 'invalid':
        mark = '还原校验失败'
    times, memory = [], []
    for key in ['compress', 'decompress']:
        phase = sample.get(key)
        if not phase:
            times.append('未执行')
            memory.append('未执行')
            continue
        seconds = phase.get('seconds')
        times.append(f'>{timeout:g}' if phase['status'] == 'timeout'
                     else f'{seconds:.3f}' if seconds is not None else '未知')
        rss = phase.get('max_rss_bytes')
        memory.append(f'{rss / 2**20:.1f}' if rss is not None else '未知')
    return {'mark': mark, 'times': times, 'memory': memory}
