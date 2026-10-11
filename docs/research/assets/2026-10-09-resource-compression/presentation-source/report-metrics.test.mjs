import assert from 'node:assert/strict';
import test from 'node:test';
import { failureDisplay, meetsHalf } from './report-metrics.mjs';

// 三轮中位数小于一半但有一轮超过：应按全部产物判定，不能把中位数当作保证。
test('50% requires every successful round and complete verification', () => {
  const rows = [49, 49, 51].map(compressed_bytes => ({ status: 'ok', compressed_bytes }));
  assert.equal(meetsHalf(rows, 100, 3), false, '有一轮超过一半必须未达标');
  rows[2].compressed_bytes = 50;
  assert.equal(meetsHalf(rows, 100, 3), true, '等于一半应达标');
  assert.equal(meetsHalf(rows.slice(0, 2), 100, 3), false, '缺轮次不能声称达标');
});

// 工具缺失与解压超时分别构造，避免把所有失败错误地写成压缩超过 300 秒。
test('failures preserve actual stage and do not invent measurements', () => {
  const absent = failureDisplay([{ status: 'unavailable' }], 300);
  assert.equal(absent.mark, '工具缺失', '缺失 CLI 应单独标注');
  assert.equal(absent.times[0], '未执行', '缺失 CLI 没有压缩耗时');
  const failed = failureDisplay([{ status: 'timeout', compress: { status: 'ok', seconds: 0.2, max_rss_bytes: 1048576 },
    decompress: { status: 'timeout', seconds: 300.01, max_rss_bytes: null } }], 300);
  assert.equal(failed.mark, '解压超时', '超时阶段应准确');
  assert.deepEqual(failed.times, [0.2, '>300'], '保留真实压缩观测');
  assert.deepEqual(failed.memory, [1, '未知'], 'RSS 缺失不能填零');
});
