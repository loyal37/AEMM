import i18n from "i18next";
import { initReactI18next } from "react-i18next";

const resources = {
  "zh-CN": {
    translation: {
      common: {
        close: "关闭",
        later: "稍后",
        next: "下一步",
        finish: "完成引导",
        working: "AEMM 正在处理后台操作",
        skipNavigation: "跳到主要内容",
      },
      navigation: {
        label: "主导航",
        dashboard: "首页",
        mods: "模组",
        profiles: "配置方案",
        settings: "设置",
      },
      topbar: {
        activeProfile: "当前配置方案",
        switchProfile: "切换当前 Profile",
        loading: "正在读取…",
        notifications: "通知",
      },
      onboarding: {
        eyebrow: "首次使用",
        title: "欢迎使用 AEMM",
        intro: "用三个步骤建立安全、可回滚的 EFMI 模组工作流。",
        securityTitle: "直接管理 EFMI Mods",
        securityBody: "AEMM 不创建第二份模组仓库；启停只在 Mods 内切换 DISABLED 文件夹名称。",
        setupTitle: "先选择 EFMI Mods",
        setupBody: "路径由 Rust 后端规范化并验证；AEMM 不检测或启动游戏。",
        profileTitle: "用 Profile 管理组合",
        profileBody: "创建角色、截图或测试配置。切换失败时，AEMM 会撤销目标部署并恢复原 Profile。",
        openSettings: "前往设置",
        step: "第 {{current}} / {{total}} 步",
      },
    },
  },
  "en-US": {
    translation: {
      common: {
        close: "Close",
        later: "Later",
        next: "Next",
        finish: "Finish setup",
        working: "AEMM is processing a background operation",
        skipNavigation: "Skip to main content",
      },
      navigation: {
        label: "Primary navigation",
        dashboard: "Dashboard",
        mods: "Mods",
        profiles: "Profiles",
        settings: "Settings",
      },
      topbar: {
        activeProfile: "Active profile",
        switchProfile: "Switch active profile",
        loading: "Loading…",
        notifications: "Notifications",
      },
      onboarding: {
        eyebrow: "Getting started",
        title: "Welcome to AEMM",
        intro: "Set up a safe EFMI Mods workflow in three steps.",
        securityTitle: "Manage EFMI Mods directly",
        securityBody: "AEMM does not create a second repository. Enable state is changed in place with the DISABLED folder prefix.",
        setupTitle: "Select EFMI Mods first",
        setupBody: "Paths are canonicalized and validated by Rust. AEMM does not detect or launch the game.",
        profileTitle: "Organize combinations with Profiles",
        profileBody: "Create character, screenshot, or test setups. Failed switches remove the target deployment and restore the original Profile.",
        openSettings: "Open Settings",
        step: "Step {{current}} of {{total}}",
      },
    },
  },
} as const;

void i18n.use(initReactI18next).init({
  resources,
  lng: "zh-CN",
  fallbackLng: "zh-CN",
  supportedLngs: ["zh-CN", "en-US"],
  interpolation: { escapeValue: false },
  returnNull: false,
});

export default i18n;
