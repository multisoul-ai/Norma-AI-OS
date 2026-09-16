import { test, expect } from '@playwright/test';
import {
  runNormaSpatialE2E,
  runNormaCanvasPointerE2E,
} from '../../../crates/e2e/browser/norma_spatial.mjs';
// 场景、数据、过程与结果见共享 runner；此入口方便 CI 或开发者复现真实浏览器测试。
test('mock agents, artifact review, and spatial controls', async ({ page }) => {
  await page.goto('/?compare=1&variant=A');
  const evidence = await runNormaSpatialE2E(page);
  expect(evidence.length, '共享流程应完成 14 项具体行为断言').toBe(14);
  const canvas = await runNormaCanvasPointerE2E(page, {
    drag: async (from: number[], to: number[]) => {
      await page.mouse.move(from[0], from[1]);
      await page.mouse.down();
      await page.mouse.move(to[0], to[1], { steps: 16 });
      await page.mouse.up();
    },
  });
  expect(canvas.length, '真实画布操作应完成 7 项断言').toBe(7);
});

import { runNormaQuietE2E } from '../../../crates/e2e/browser/norma_quiet.mjs';
test('content-first canvas progressively discloses controls', async ({
  page,
}) => {
  await page.goto('/?mode=workspace');
  const evidence = await runNormaQuietE2E(page, {
    drag: async (from: number[], to: number[]) => {
      await page.mouse.move(from[0], from[1]);
      await page.mouse.down();
      await page.mouse.move(to[0], to[1], { steps: 16 });
      await page.mouse.up();
    },
  });
  expect(evidence.length, '精简画布应完成 20 项真实交互断言').toBe(20);
});

import { runNormaVRPreviewE2E } from '../../../crates/e2e/browser/norma_quiet.mjs';
test('default entry is the VR spatial prototype', async ({ page }) => {
  await page.goto('/');
  const evidence = await runNormaVRPreviewE2E(page);
  expect(evidence.length, '默认 VR 入口完成 3 项结构断言').toBe(3);
});

import { runNormaVRWorkflowE2E } from '../../../crates/e2e/browser/norma_vr.mjs';
test.describe('VR execution workspace', () => {
  test.use({ viewport: { width: 1280, height: 720 } });
  test('multiple task decisions stay reachable and resolve independently', async ({
    page,
  }) => {
    await page.goto('/?mode=orbit');
    const evidence = await runNormaVRWorkflowE2E(page, {
      click: async (point: number[]) => page.mouse.click(point[0], point[1]),
    });
    expect(evidence.length, 'VR 多任务队列应完成十三项决策断言').toBe(13);
  });
});
