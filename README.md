# Endfield Mod Manager

Endfield Mod Manager（AEMM）是一个面向 Windows 10/11 的《明日方舟：终末地》桌面模组管理器。项目目标是提供安全、可回滚且可扩展的本地模组仓库、部署策略、冲突分析和 Profile，而不是把加载方式绑定到某个尚未验证的游戏版本。

> 当前状态：Phase 2 游戏路径管理已完成。应用可以检测/验证国服安装、配置 EFMI、打开游戏目录并按安全启动规格启动；模组扫描与安装会按后续路线图实现。

## 技术栈

- Tauri 2 + Rust 2024
- React 19 + TypeScript + Vite
- SQLx + SQLite（WAL、外键、嵌入式 migration）
- Tokio、Serde、thiserror、tracing
- React Router、TanStack Query、Lucide

## 本地开发

### 环境要求

- Windows 10 或 Windows 11
- Node.js 22+
- pnpm 11+
- Rust stable（MSVC toolchain，包含 `rustfmt` 与 `clippy`）
- Visual Studio 2022 C++ Build Tools
- Microsoft Edge WebView2 Runtime

### 启动

```powershell
pnpm install
pnpm tauri dev
```

项目脚本也兼容 npm：

```powershell
npm install
npm run tauri dev
```

### 质量检查

```powershell
pnpm build
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

## 架构概览

React 只通过窄化、类型化的 Tauri command 调用应用用例。Command 不承载业务逻辑；游戏适配、模组扫描/安装、部署、冲突与 Profile 都位于独立核心模块中。SQLite 保存关系型业务数据，`config.json` 保存机器相关路径和偏好。

模组启用遵循“仓库内容 → 部署策略 → 游戏/加载器目标”的模型。禁用只撤销部署记录，不删除仓库中的模组本体。具体 EFMI、复制、硬链接、符号链接或配置编辑行为由 `ModDeploymentStrategy` 实现，通用业务不写死某一种方案。

详细说明见 [ARCHITECTURE.md](ARCHITECTURE.md)，当前上下文与已知问题见 [PROJECT_CONTEXT.md](PROJECT_CONTEXT.md)，开发路线见 [TASKS.md](TASKS.md)。

## EFMI 适配状态

已对本地 EFMI/3DMigoto 布局进行只读分析，并实现结构与启动路径验证：它以 `Endfield.exe` 为目标、递归包含 `Mods`，并排除 `DISABLED*` 目录。当前 EFMI 的 `launch` 指向旧游戏位置，因此会显示“结构有效但不可启动”，不会盲目执行。国际服布局、XXMI 启动协议、加载顺序和 INI 级冲突语义仍需真实夹具验证。

## 安全原则

- 不执行第三方加载器或模组二进制进行扫描。
- 安装阶段必须拒绝 Zip Slip、绝对/设备路径、危险链接、大小/数量异常和 Windows 保留名称。
- 删除或撤销部署前必须证明目标位于 AEMM 允许管理的根目录内，并且不能删除根目录本身。
- 安装和 Profile 切换采用可记录、可验证、可回滚的计划。

## 项目状态

项目尚未发布稳定版本，也尚未选择开源许可证。在许可证文件加入仓库前，请勿假定获得了源码再分发授权。
