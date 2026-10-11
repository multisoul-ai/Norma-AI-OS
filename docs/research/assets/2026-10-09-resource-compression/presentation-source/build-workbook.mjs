import fs from 'node:fs/promises';
import path from 'node:path';
import assert from 'node:assert/strict';
import { Workbook, SpreadsheetFile } from '@oai/artifact-tool';
import { failureDisplay, meetsHalf } from './report-metrics.mjs';

const [matrixFile, dmgFile, outputDir] = process.argv.slice(2);
if (!matrixFile || !dmgFile || !outputDir) throw new Error('参数：results.json dmg-results.json outputDir');
const data = JSON.parse(await fs.readFile(matrixFile, 'utf8'));
const dmg = JSON.parse(await fs.readFile(dmgFile, 'utf8'));
assert(data.finished_utc && data.source_unchanged, '只交付完整且源文件未变的实验');
assert(dmg.finished_utc && dmg.source_unchanged && dmg.results.length === 4, '四组 DMG 对照必须完成');
const groups = { resources: '完整资源包', video: '视频', images: '图像' };
const cases = data.configuration.algorithms;
const repeats = data.configuration.repeats;
const median = (items) => {
  const sorted = [...items].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
};
const workbook = Workbook.create();
const report = workbook.worksheets.add('选型表');
const raw = workbook.worksheets.add('逐轮数据');
const navy = '#253D63', green = '#E6F0E8', amber = '#FFF1D6', red = '#FBE8E7';
function base(sheet, range) {
  sheet.showGridLines = false;
  sheet.getRange(range).format.font = { name: 'Helvetica Neue', size: 11, color: '#243247' };
  sheet.getRange(range).format.verticalAlignment = 'center';
  sheet.getRange(range).format.rowHeight = 24;
}
function title(sheet, cell, value) {
  sheet.getRange(cell).values = [[value]];
  sheet.getRange(cell).format.font = { name: 'Helvetica Neue', size: 16, bold: true, color: navy };
  sheet.getRange(cell).format.rowHeight = 32;
}
function header(sheet, range) {
  sheet.getRange(range).format = {
    fill: navy, font: { name: 'Helvetica Neue', size: 11, color: '#FFFFFF', bold: true },
    horizontalAlignment: 'center', verticalAlignment: 'center', wrapText: true,
  };
  sheet.getRange(range).format.rowHeight = 44;
}
function table(sheet, range, name) {
  const t = sheet.tables.add(range, true, name);
  t.showFilterButton = true;
  t.style = 'TableStyleMedium2';
  return t;
}
const rawStart = 7;
const rawEnd = rawStart + data.results.length - 1;
base(raw, `A1:Q${rawEnd + 13}`);
title(raw, 'A1', '压缩实验原始观测');
raw.getRange('A2').values = [['来源：results.json（核对输入等价性后合并的逐轮记录）']];
raw.getRange('A3').values = [[`${data.environment.cpu} / ${Number(data.environment.memory_bytes) / 2 ** 30} GiB / ${data.environment.platform}`]];
raw.getRange('A4').values = [[`单压缩线程，阶段超时 ${data.configuration.timeout_seconds} 秒。失败后的剩余轮次跳过。RSS 单位为 bytes。`]];
raw.getRange('A6:P6').values = [[
  '数据集', '算法参数', '轮次', '状态', '原文件 bytes', '统一 tar bytes', '压缩产物 bytes',
  '压缩秒', '解压秒', '压缩峰值 RSS bytes', '解压峰值 RSS bytes', '核验文件数',
  '预期文件数', '解压及核验秒', '压缩产物 SHA-256', '统一 tar SHA-256',
]];
const rawRows = data.results.map((row) => [
  groups[row.dataset], row.algorithm, row.iteration, row.status,
  data.datasets[row.dataset].source_bytes, data.datasets[row.dataset].tar_bytes,
  row.compressed_bytes ?? null, row.compress?.seconds ?? null, row.decompress?.seconds ?? null,
  row.compress?.max_rss_bytes ?? null, row.decompress?.max_rss_bytes ?? null,
  row.verified_files, data.datasets[row.dataset].file_count,
  row.decompress_and_verify_seconds ?? null, row.compressed_sha256 ?? null,
  data.datasets[row.dataset].tar_sha256,
]);
raw.getRange(`A${rawStart}:P${rawEnd}`).values = rawRows;
table(raw, `A6:P${rawEnd}`, 'CompressionObservations');
header(raw, 'A6:P6');
raw.getRange(`C7:G${rawEnd}`).setNumberFormat('#,##0');
raw.getRange(`H7:I${rawEnd}`).setNumberFormat('0.000');
raw.getRange(`J7:M${rawEnd}`).setNumberFormat('#,##0');
raw.getRange(`N7:N${rawEnd}`).setNumberFormat('0.000');
raw.getRange(`C7:D${rawEnd}`).format.horizontalAlignment = 'center';
raw.getRange(`L7:M${rawEnd}`).format.horizontalAlignment = 'center';
raw.getRange('A1:A120').format.columnWidth = 17;
raw.getRange('B1:B120').format.columnWidth = 20;
raw.getRange('C1:D120').format.columnWidth = 12;
raw.getRange('E1:N120').format.columnWidth = 19;
raw.getRange('O1:P120').format.columnWidth = 68;
raw.freezePanes.freezeRows(6);
raw.freezePanes.freezeColumns(2);
raw.getRange(`D7:D${rawEnd}`).conditionalFormats.add('containsText', { text: 'timeout', format: { fill: amber, font: { color: '#8C5500' } } });

base(report, 'A1:P70');
report.tabColor = navy;
title(report, 'A2', 'Norma OS 无损压缩选型');
report.getRange('A3:L3').format.borders = { bottom: { style: 'thin', color: navy } };
report.getRange('A4').values = [['逐字节还原；统计成功轮次中位数与最大 RSS。排名仅含 300 秒期限内三轮全通过的候选。']];
report.getRange('A5').values = [['体积采用十进制 MB，内存采用 MiB。净节省计入归档开销，负值表示膨胀。']];
report.getRange('A11').values = [['解压计时只恢复统一 tar，不包含逐文件写入缓存及 SHA-256 核验。']];
report.getRange('A4:A5').format.font = { name: 'Helvetica Neue', size: 11, italic: true, color: '#586779' };
report.getRange('A7:H7').values = [['范围', '原始 MB', '体积最小算法', '最大净节省', '最快压缩', '最快解压', '50% 目标', '成功 / 超时']];
header(report, 'A7:H7');
const mainStart = 15, mainEnd = 50;
report.getRange('A14:P14').values = [[
  '数据集', '算法参数', '压缩体积 MB', '净节省率', '压缩秒', '解压秒',
  '压缩峰值 MiB', '解压峰值 MiB', '核验轮数', '体积排名', '选择标注', '原文件 bytes',
  '产物中位数 bytes', '压缩峰值 RSS bytes', '解压峰值 RSS bytes', '产物最大值 bytes',
]];
table(report, `A14:P${mainEnd}`, 'CompressionComparison');
header(report, 'A14:P14');
report.freezePanes.freezeRows(14);
report.freezePanes.freezeColumns(2);
const mainRows = [];
const indexByGroup = {};
for (const group of Object.keys(groups)) {
  indexByGroup[group] = [];
  for (const algorithm of cases) {
    const samples = data.results.map((row, i) => ({ ...row, rawRow: rawStart + i }))
      .filter(row => row.dataset === group && row.algorithm === algorithm);
    const valid = samples.filter(row => row.status === 'ok');
    const complete = valid.length === repeats && samples.length === repeats;
    const r = mainStart + mainRows.length;
    const failure = failureDisplay(samples, data.configuration.timeout_seconds);
    const status = valid.length ? `${valid.length}/${repeats}` : failure.mark;
    report.getRange(`A${r}:B${r}`).values = [[groups[group], algorithm]];
    report.getRange(`I${r}`).values = [[status]];
    report.getRange(`L${r}`).values = [[data.datasets[group].source_bytes]];
    if (valid.length) {
      report.getRange(`M${r}:O${r}`).values = [[median(valid.map(s => s.compressed_bytes)), Math.max(...valid.map(s => s.compress.max_rss_bytes)), Math.max(...valid.map(s => s.decompress.max_rss_bytes))]];
      report.getRange(`P${r}`).values = [[Math.max(...valid.map(s => s.compressed_bytes))]];
      report.getRange(`C${r}:D${r}`).formulas = [[`=M${r}/1000000`, `=1-M${r}/L${r}`]];
      report.getRange(`E${r}:F${r}`).values = [[median(valid.map(s => s.compress.seconds)), median(valid.map(s => s.decompress.seconds))]];
      report.getRange(`G${r}:H${r}`).formulas = [[`=N${r}/1048576`, `=O${r}/1048576`]];
    } else {
      report.getRange(`C${r}:H${r}`).values = [['未知', '未知', ...failure.times, ...failure.memory]];
    }
    if (complete && algorithm !== 'store') {
      report.getRange(`J${r}`).formulas = [[`=COUNTIFS($A$${mainStart}:$A$${mainEnd},A${r},$I$${mainStart}:$I$${mainEnd},"${repeats}/${repeats}",$B$${mainStart}:$B$${mainEnd},"<>store",$C$${mainStart}:$C$${mainEnd},"<"&C${r})+1`]];
      const candidate = group === 'resources' && algorithm === 'zstd-19';
      report.getRange(`K${r}`).formulas = [[`=IF(J${r}=1,"体积最小","${candidate ? '综合归档候选' : '已验证'}")`]];
    } else {
      report.getRange(`K${r}`).values = [[algorithm === 'store' ? '归档基线' : `排除：${failure.mark}`]];
    }
    mainRows.push({ group, algorithm, r, valid, complete });
    indexByGroup[group].push(mainRows.at(-1));
  }
}
for (const [i, group] of Object.keys(groups).entries()) {
  const r = 8 + i;
  const good = indexByGroup[group].filter(s => s.complete && s.algorithm !== 'store');
  const winner = good.reduce((a, b) => median(a.valid.map(s => s.compressed_bytes)) <= median(b.valid.map(s => s.compressed_bytes)) ? a : b);
  const speed = phase => good.reduce((a, b) => median(a.valid.map(s => s[phase].seconds)) <= median(b.valid.map(s => s[phase].seconds)) ? a : b);
  const rows = data.results.filter(s => s.dataset === group);
  report.getRange(`A${r}`).values = [[groups[group]]];
  report.getRange(`B${r}`).formulas = [[`=SUMIFS($L$${mainStart}:$L$${mainEnd},$A$${mainStart}:$A$${mainEnd},A${r},$B$${mainStart}:$B$${mainEnd},C${r})/1000000`]];
  report.getRange(`C${r}`).values = [[winner.algorithm]];
  report.getRange(`D${r}`).formulas = [[`=SUMIFS($D$${mainStart}:$D$${mainEnd},$A$${mainStart}:$A$${mainEnd},A${r},$B$${mainStart}:$B$${mainEnd},C${r})`]];
  report.getRange(`E${r}:F${r}`).values = [[speed('compress').algorithm, speed('decompress').algorithm]];
  report.getRange(`G${r}`).formulas = [[`=IF(SUMIFS($P$${mainStart}:$P$${mainEnd},$A$${mainStart}:$A$${mainEnd},A${r},$B$${mainStart}:$B$${mainEnd},C${r})*2<=SUMIFS($L$${mainStart}:$L$${mainEnd},$A$${mainStart}:$A$${mainEnd},A${r},$B$${mainStart}:$B$${mainEnd},C${r}),"达到","未达到")`]];
  report.getRange(`H${r}`).values = [[`${rows.filter(s => s.status === 'ok').length} / ${rows.filter(s => s.status === 'timeout').length}`]];
}
report.getRange('A12').values = [['综合建议：保留原媒体资源；先评估 ULMO DMG。预压缩后的额外收益见下方 DMG 实测。']];
report.getRange('A12').format.font = { name: 'Helvetica Neue', size: 11, bold: true, color: navy };
report.getRange(`C${mainStart}:C${mainEnd}`).setNumberFormat('0.000');
report.getRange(`C${mainStart}:H${mainEnd}`).format.horizontalAlignment = 'right';
report.getRange(`D${mainStart}:D${mainEnd}`).setNumberFormat('0.000%');
report.getRange(`E${mainStart}:F${mainEnd}`).setNumberFormat('0.000');
report.getRange(`G${mainStart}:H${mainEnd}`).setNumberFormat('0.0');
report.getRange(`L${mainStart}:P${mainEnd}`).setNumberFormat('#,##0');
report.getRange('B8:B10').setNumberFormat('0.000');
report.getRange('D8:D10').setNumberFormat('0.000%');
report.getRange('C8:C10').format.horizontalAlignment = 'center';
report.getRange('E8:H10').format.horizontalAlignment = 'center';
report.getRange(`I${mainStart}:K${mainEnd}`).format.horizontalAlignment = 'center';
report.getRange('A1:A70').format.columnWidth = 17;
report.getRange('B1:B70').format.columnWidth = 20;
report.getRange('C1:D70').format.columnWidth = 20;
report.getRange('E1:H70').format.columnWidth = 17;
report.getRange('I1:K70').format.columnWidth = 16;
report.getRange('L1:P70').format.columnWidth = 24;
report.getRange(`A${mainStart}:K${mainEnd}`).conditionalFormats.addCustom(`=$K${mainStart}="体积最小"`, { fill: green, font: { bold: true } });
report.getRange(`A${mainStart}:K${mainEnd}`).conditionalFormats.addCustom(`=ISNUMBER(SEARCH("超时",$I${mainStart}))`, { fill: amber, font: { color: '#8C5500' } });
report.getRange(`K${mainStart}:K${mainEnd}`).conditionalFormats.add('containsText', { text: '排除', format: { fill: amber, font: { color: '#8C5500' } } });
report.getRange(`D${mainStart}:D${mainEnd}`).conditionalFormats.add('cellIs', { operator: 'lessThan', formula: 0, format: { fill: red, font: { color: '#A13E35' } } });

title(report, 'A53', 'DMG 对照：固定模拟载荷');
report.getRange('A54').values = [['相同程序快照与资源；不是当前正式发布包。每组一次，镜像验证及挂载 SHA-256 通过。']];
report.getRange('A55').values = [['制作峰值为 hdiutil CLI RSS，不代表镜像服务与内核缓存的总内存。']];
report.getRange('A56:J56').values = [['资源载荷', '镜像格式', '镜像 MB', '预压缩净节省', '制作秒', '制作峰值 MiB', '恢复文件数', '镜像 / 文件核验', '镜像 bytes', '制作峰值 RSS bytes']];
const dmgRawHeader = rawEnd + 4, dmgRawStart = dmgRawHeader + 1;
raw.getRange(`A${dmgRawHeader - 2}`).values = [['来源：dmg-results.json（固定模拟载荷，四组各一次）']];
raw.getRange(`A${dmgRawHeader - 1}`).values = [['制作峰值为 hdiutil CLI RSS；不代表整个镜像制作流程的总内存。']];
raw.getRange(`A${dmgRawHeader}:H${dmgRawHeader}`).values = [['资源载荷', '镜像格式', '镜像 bytes', '制作秒', '制作峰值 RSS bytes', '恢复文件数', '挂载核验', '镜像 SHA-256']];
raw.getRange(`A${dmgRawStart}:H${dmgRawStart + 3}`).values = dmg.results.map(row => [row.payload, row.format, row.bytes, row.create.seconds, row.create.max_rss_bytes, row.verified_resource_files, row.payload_sha256_matches, row.sha256]);
table(raw, `A${dmgRawHeader}:H${dmgRawStart + 3}`, 'DmgObservations');
header(raw, `A${dmgRawHeader}:H${dmgRawHeader}`);
raw.getRange(`C${dmgRawStart}:C${dmgRawStart + 3}`).setNumberFormat('#,##0');
raw.getRange(`D${dmgRawStart}:D${dmgRawStart + 3}`).setNumberFormat('0.000');
raw.getRange(`E${dmgRawStart}:F${dmgRawStart + 3}`).setNumberFormat('#,##0');
dmg.results.forEach((row, i) => {
  const r = 57 + i;
  report.getRange(`A${r}:B${r}`).values = [[row.payload === 'plain' ? '原资源' : dmg.resource_algorithm, row.format]];
  report.getRange(`I${r}:J${r}`).values = [[row.bytes, row.create.max_rss_bytes]];
  report.getRange(`C${r}:D${r}`).formulas = [[`=I${r}/1000000`, `=1-I${r}/SUMIFS($I$57:$I$60,$B$57:$B$60,B${r},$A$57:$A$60,"原资源")`]];
  report.getRange(`E${r}`).values = [[row.create.seconds]];
  report.getRange(`F${r}`).formulas = [[`=J${r}/1048576`]];
  report.getRange(`G${r}`).values = [[row.verified_resource_files]];
  report.getRange(`H${r}`).values = [['通过']];
});
table(report, 'A56:J60', 'DmgComparison');
header(report, 'A56:J56');
report.getRange('C57:C60').setNumberFormat('0.000');
report.getRange('D57:D60').setNumberFormat('0.000%');
report.getRange('E57:E60').setNumberFormat('0.000');
report.getRange('F57:F60').setNumberFormat('0.0');
report.getRange('C57:G60').format.horizontalAlignment = 'right';
report.getRange('G57:H60').format.horizontalAlignment = 'center';
report.getRange('I57:J60').setNumberFormat('#,##0');
report.getRange('A62').values = [[`安装后还原仍占 ${(data.datasets.resources.source_bytes / 1e6).toFixed(3)} MB；保留压缩包时还需加计归档体积。`]];
workbook.recalculate();
for (const [i, group] of Object.keys(groups).entries()) {
  const winner = report.getRange(`C${8 + i}`).values[0][0];
  const samples = data.results.filter(row => row.dataset === group && row.algorithm === winner);
  assert.equal(report.getRange(`G${8 + i}`).values[0][0],
    meetsHalf(samples, data.datasets[group].source_bytes, repeats) ? '达到' : '未达到', '50% 判定必须与所有轮次一致');
}
for (const item of mainRows) {
  if (!item.complete) continue;
  const values = report.getRange(`C${item.r}:H${item.r}`).values[0];
  const expected = [median(item.valid.map(s => s.compressed_bytes)) / 1e6,
    1 - median(item.valid.map(s => s.compressed_bytes)) / data.datasets[item.group].source_bytes,
    median(item.valid.map(s => s.compress.seconds)), median(item.valid.map(s => s.decompress.seconds)),
    Math.max(...item.valid.map(s => s.compress.max_rss_bytes)) / 2 ** 20,
    Math.max(...item.valid.map(s => s.decompress.max_rss_bytes)) / 2 ** 20];
  values.forEach((value, i) => assert(Math.abs(value - expected[i]) < 1e-8, `${item.group}/${item.algorithm} 第 ${i} 列错误`));
}
const errors = await workbook.inspect({ kind: 'match', searchTerm: '#REF!|#DIV/0!|#VALUE!|#NAME\\?|#N/A|#NUM!|#NULL!|#SPILL!|#CALC!', options: { useRegex: true, maxResults: 50 }, maxChars: 3000 });
console.log(errors.ndjson);
const inspection = await workbook.inspect({ kind: 'table', range: '选型表!A7:H10', include: 'values,formulas', tableMaxRows: 4, tableMaxCols: 8, maxChars: 4500 });
console.log(inspection.ndjson);
await fs.mkdir(outputDir, { recursive: true });
for (const [name, sheetName, range] of [
  ['selection', '选型表', 'A1:K27'], ['video-images', '选型表', 'A27:L50'],
  ['dmg', '选型表', 'A53:H62'], ['observations', '逐轮数据', 'A1:N17'],
]) {
  const preview = await workbook.render({ sheetName, range, scale: 1, format: 'png' });
  await fs.writeFile(path.join(outputDir, `${name}.png`), new Uint8Array(await preview.arrayBuffer()));
}
await (await SpreadsheetFile.exportXlsx(workbook)).save(path.join(outputDir, 'Norma-无损压缩算法选型.xlsx'));
console.log(JSON.stringify({ output: path.join(outputDir, 'Norma-无损压缩算法选型.xlsx'), observations: data.results.length, completedComparisons: mainRows.filter(s => s.complete).length }));
