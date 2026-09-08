# monorepo 与私有仓库

唯一开发源为 `multisoul-ai/Norma-AI-OS` 中的 `apps/quest/`。

`multisoul-ai/Norma-Quest` 是该目录的独立私有导出，包含完整 Gradle 工程、协议、mock 服务、文档和自己的 GitHub Actions。它可以独立克隆构建，但后续开发应回到 monorepo，避免分叉。

采用 **Git subtree split**，不是 submodule，也没有嵌套 `.git`。因此主仓库正常 clone 就有 Quest 全部源码。

从 monorepo 推送已验证的 Quest 版本：

```sh
bash scripts/quest.sh check
quest_revision=$(git subtree split --prefix=apps/quest)
git push https://github.com/multisoul-ai/Norma-Quest.git "${quest_revision}:refs/heads/main"
```

这些步骤发布当前 HEAD 中的已提交文件，无需创建临时分支。正常使用快进推送；若目标存在独立修改导致分叉，应先审查并将修改移回 monorepo，不要 force push。

主仓库集成通过开发分支与 PR 审查；初次上传不自动合并到 `dev/main`。独立导出只包含 `apps/quest` 历史，不复制 Mac 其他源码或工作区中的未提交原型。
