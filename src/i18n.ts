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
        currentGame: "当前游戏",
        gameName: "明日方舟：终末地",
        configured: "游戏目录已验证",
        waiting: "等待设置路径",
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
        intro: "用三个步骤建立安全、可回滚的终末地模组工作流。",
        securityTitle: "原始模组始终保留",
        securityBody: "AEMM 将模组保存在独立仓库，再通过所有权清单部署到 EFMI；禁用不会删除仓库本体。",
        setupTitle: "先验证游戏与 EFMI",
        setupBody: "所有路径都由 Rust 后端重新规范化和验证，前端不能指定任意部署或删除目标。",
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
        currentGame: "Current game",
        gameName: "Arknights: Endfield",
        configured: "Game directory verified",
        waiting: "Waiting for setup",
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
        intro: "Set up a safe, rollback-capable Endfield mod workflow in three steps.",
        securityTitle: "Original mods stay intact",
        securityBody: "AEMM keeps mods in its own repository and deploys them to EFMI with ownership manifests. Disabling never deletes the repository copy.",
        setupTitle: "Verify the game and EFMI first",
        setupBody: "Every path is canonicalized and validated by the Rust backend. The frontend cannot choose arbitrary deployment or deletion targets.",
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
