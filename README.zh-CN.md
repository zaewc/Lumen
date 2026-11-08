<div align="center">

<img src="docs/assets/readme/thumbnail.webp" width="960" alt="Lumen — 适用于各种设备的存储空间管理。笔记本电脑模型中的应用预览。">

# Lumen

**适用于各种设备的存储空间管理。**

Lumen 会找出占用磁盘空间的内容，并说明每个项目是什么。只有确认可以安全
清理的项目才会移入可恢复的隔离区。

[![最新发行版本](https://img.shields.io/github/v/release/zaewc/Lumen?label=beta&color=4f46e5)](https://github.com/zaewc/Lumen/releases/latest)
[![Rust](https://github.com/zaewc/Lumen/actions/workflows/rust.yml/badge.svg)](https://github.com/zaewc/Lumen/actions/workflows/rust.yml)
[![许可证：Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

[**下载**](https://zaewc.github.io/Lumen/zh-CN/) · [工作原理](ARCHITECTURE.md) · [路线图](docs/architecture/roadmap.md)

<!-- language-tabs:start -->
<p>
<a href="README.ko.md" lang="ko" hreflang="ko"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/language-tabs/ko-dark.svg"><img src="docs/assets/readme/language-tabs/ko-light.svg" width="88" height="44" alt="한국어"></picture></a><a href="README.md" lang="en" hreflang="en"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/language-tabs/en-dark.svg"><img src="docs/assets/readme/language-tabs/en-light.svg" width="88" height="44" alt="English"></picture></a><a href="README.ru.md" lang="ru" hreflang="ru"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/language-tabs/ru-dark.svg"><img src="docs/assets/readme/language-tabs/ru-light.svg" width="88" height="44" alt="Русский"></picture></a><a href="README.ja.md" lang="ja" hreflang="ja"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/language-tabs/ja-dark.svg"><img src="docs/assets/readme/language-tabs/ja-light.svg" width="88" height="44" alt="日本語"></picture></a><a href="README.zh-CN.md" lang="zh-CN" hreflang="zh-CN"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/language-tabs/zh-CN-selected-dark.svg"><img src="docs/assets/readme/language-tabs/zh-CN-selected-light.svg" width="88" height="44" alt="简体中文 ✓"></picture></a><a href="README.es.md" lang="es" hreflang="es"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/language-tabs/es-dark.svg"><img src="docs/assets/readme/language-tabs/es-light.svg" width="88" height="44" alt="Español"></picture></a><a href="README.fr.md" lang="fr" hreflang="fr"><picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/language-tabs/fr-dark.svg"><img src="docs/assets/readme/language-tabs/fr-light.svg" width="88" height="44" alt="Français"></picture></a>
</p>
<!-- language-tabs:end -->

![语言统计](https://github-repository-language-graph-wi.vercel.app/api?username=zaewc&repo=Lumen&theme=dark&precision=6)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/clean-up-dark.png">
  <img src="docs/assets/readme/clean-up-light.png" width="840" alt="Lumen「探索」页面示例：按决策分类的存储空间、建议清理的 9.9 GB，以及由工具自身清理项目的命令。">
</picture>

</div>

## Lumen 的功能

- **找出占用空间的内容**，包括 Xcode DerivedData、Gradle 缓存、应用缓存和旧
  安装包；扫描时不会打开任何文件。
- **解释每项决策。** 每个项目会被保留、留给你检查，或建议清理，并附上相关
  证据：项目来源、能否重新生成，以及是否仍在使用。
- **绝不擅自删除。** 清理时，项目会移到同一磁盘上的隔离区；你可以恢复它们，
  只有你确认后才会彻底删除。
- **判断何时应由工具自身清理。** 对于 pnpm 存储、Xcode 模拟器等内容，Lumen
  会显示相应工具的清理命令，而不是直接操作文件。
- **提供 AI 第二意见。** AI 检查会说明需要检查的项目，并且只能让决策变得
  更谨慎。
- **可在应用内更新**（从 0.5.0 版本开始）。安装前会使用 Lumen 的签名密钥
  验证新版本。

## 进一步了解

<table>
  <tr>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/review-dark.png">
        <img src="docs/assets/readme/review-light.png" alt="项目证据检查器：来源、位置、检查决策及原因，并可选显示 AI 备注。示例数据。">
      </picture>
      <p align="center"><b>证据</b>：项目来源及其决策依据</p>
    </td>
    <td width="50%">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/overview-dark.png">
        <img src="docs/assets/readme/overview-light.png" alt="示例存储空间页面：可用磁盘空间、测量容量视图，以及按决策分组的最近一次扫描结果。">
      </picture>
      <p align="center"><b>存储空间</b>：磁盘容量和最近一次扫描的分类结果</p>
    </td>
  </tr>
  <tr>
    <td colspan="2">
      <picture>
        <source media="(prefers-color-scheme: dark)" srcset="docs/assets/readme/quarantine-dark.png">
        <img src="docs/assets/readme/quarantine-light.png" alt="隔离区：两个暂存项目，其中一个已选中；可选择恢复或彻底删除。">
      </picture>
      <p align="center"><b>隔离区</b>：恢复项目，或一次彻底删除多个项目</p>
    </td>
  </tr>
</table>

## Lumen 如何作出决策

所有决策都由确定性的安全策略作出，而不是由模型决定：

| 决策 | 含义 |
| --- | --- |
| **保留** | 系统文件、文档、应用数据、正在使用或从云端同步的项目，以及 Lumen 无法完整读取的内容。 |
| **检查** | Lumen 无法确定，由你来决定。 |
| **清理** | 已知且可重新生成的项目，例如构建缓存。它们会先移入隔离区。 |

如果无法确定，Lumen 会保留文件或请你检查。移动前会再次确认项目未发生变化且
未在使用，并通过文件句柄操作，而不是使用之前查询到的路径。具体规则见
[扫描安全约定](docs/architecture/scanner.md)和[决策记录](docs/decisions/README.md)。

## AI 检查

**首次启动时，Lumen 会询问一次是否启用 AI 检查。** 在你选择之前不会发送任何
内容；之后也可以随时在设置中更改
([ADR-0050](docs/decisions/0050-ask-about-ai-review-on-first-launch.md))。启用后，
Lumen 会向学校 AI Hub 的 Jeb 模型发送建议清理的项目，以及最多 20 个较大的
待检查项目的元数据。元数据包括路径（主目录以 `~` 表示）、文件名、大小、
时间信息和决策依据。服务运营方可能会保留请求。**绝不会发送文件内容。**
AI 的回答仅作为证据：它可以让决策更谨慎，但不能降低谨慎程度，也不会选择要
移动或删除的项目
([ADR-0020](docs/decisions/0020-jev-evidence-only-judge.md))。准确度及评估方式
见 [`docs/ai/`](docs/ai/architecture.md)。

## 安装

从[下载页面](https://zaewc.github.io/Lumen/zh-CN/)或
[GitHub Releases](https://github.com/zaewc/Lumen/releases/latest)下载最新测试版。

| 平台 | 安装包 | 状态 |
| --- | --- | --- |
| macOS 13 或更高版本（Apple silicon 和 Intel） | `.dmg` | 测试版 |
| Windows 10 22H2 或更高版本、Windows 11 | 安装程序（`-setup.exe`） | 测试版 |
| Linux（x86_64） | `.AppImage`、`.deb` | 测试版 |
| Android、iOS | | 开发中 |

测试版尚未进行代码签名，因此首次启动时需要额外操作：在 macOS 上选择
**系统设置 › 隐私与安全性 › 仍要打开**；在 Windows 上选择
**更多信息 › 仍要运行**。每个文件都附带 SHA-256 校验和及构建来源证明：

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing
gh attestation verify <file> --repo zaewc/Lumen
```

## 从源代码构建

需要 Rust（版本见 `rust-toolchain.toml`）、包含 Corepack 的 Node.js 24，以及
[`just`](https://github.com/casey/just)。

```sh
corepack enable && pnpm install
pnpm --filter @lumen/desktop tauri dev   # 桌面应用
just check                               # 格式、lint、测试、cargo-deny、schema 和 Web 检查
cargo run -p lumen-cli -- scan ~/Library --db index.db   # 只读命令行扫描
```

提交拉取请求前，请阅读 [`CONTRIBUTING.md`](CONTRIBUTING.md) 和
[`AGENTS.md`](AGENTS.md)。

## 文档

| 主题 | 文档 |
| --- | --- |
| 架构 | [`ARCHITECTURE.md`](ARCHITECTURE.md)、[系统](docs/architecture/system.md)、[平台](docs/architecture/platforms.md) |
| 决策 | [`docs/decisions/`](docs/decisions/README.md) |
| 路线图 | [`docs/architecture/roadmap.md`](docs/architecture/roadmap.md) |
| AI 检查 | [`docs/ai/`](docs/ai/architecture.md) |
| 隐私 | [`docs/operations/privacy.md`](docs/operations/privacy.md) |
| 安全 | [`SECURITY.md`](SECURITY.md)、[威胁模型](docs/security/threat-model.md) |

## 原则

- **安全优先于释放空间。** 无法确定时，Lumen 会保留文件或请你检查。
- **以证据为依据，不靠猜测。** 每项建议都有可解释的理由。
- **AI 只提供建议。** 模型输出是证据，最终决策由确定性策略作出。
- **默认可撤销。** 删除前先隔离并验证，且支持恢复。
- **本地优先。** 扫描、决策和隔离区都保留在你的设备上；文件内容不会离开设备。

## 许可证

Lumen 根据 [Apache License, Version 2.0](LICENSE) 授权。
