# Norma iOS 原型静态站

把 `docs/prototypes` 及相对路径依赖的九屏图 / 虚拟人肖像打成可部署静态根目录。

## 入口

- 主入口：`/prototypes/norma-ios-workspace.html`
- 备用：`/prototypes/norma-realtime.html`
- `/` 会临时跳到主入口

## 本地准备

```bash
cd sites/norma-ios-prototype
npm run prepare:static
python3 -m http.server 8765 --directory dist
```

## 部署到 Vercel

需要本机或 CI 已登录 Vercel（`vercel login` / `VERCEL_TOKEN`），并具备团队 `multisoul-ai-s-projects`（或等价）的创建-project 权限。

```bash
cd sites/norma-ios-prototype
# 预览
npm run deploy
# 生产
npm run deploy:prod
```

首次部署后可在 Vercel 控制台绑定稳定域名（例如 `norma-ios-prototype.vercel.app`）。
