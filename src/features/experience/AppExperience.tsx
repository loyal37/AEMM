import { useIsFetching, useIsMutating } from "@tanstack/react-query";
import { FolderCheck, Layers3, ShieldCheck, X } from "lucide-react";
import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { commandErrorMessage } from "../../lib/tauri";
import { useAppBootstrap, useUpdateAppSettings } from "../bootstrap/useAppBootstrap";
import { useDialogFocus } from "./useDialogFocus";

interface OnboardingRequest {
  requested: boolean;
  open: () => void;
  close: () => void;
}

const OnboardingContext = createContext<OnboardingRequest | null>(null);

export function OnboardingProvider({ children }: { children: ReactNode }) {
  const [requested, setRequested] = useState(false);
  const value = useMemo(
    () => ({ requested, open: () => setRequested(true), close: () => setRequested(false) }),
    [requested],
  );
  return <OnboardingContext.Provider value={value}>{children}</OnboardingContext.Provider>;
}

export function useOnboarding() {
  const value = useContext(OnboardingContext);
  if (!value) throw new Error("useOnboarding must be used inside OnboardingProvider");
  return value;
}

export function ExperienceController() {
  const bootstrap = useAppBootstrap();
  const { i18n } = useTranslation();
  const settings = bootstrap.data?.settings;

  useEffect(() => {
    if (!settings) return;
    document.documentElement.lang = settings.language;
    void i18n.changeLanguage(settings.language);

    const media = window.matchMedia("(prefers-color-scheme: light)");
    const applyTheme = () => {
      const resolved = settings.theme === "system" && media.matches ? "light" : "dark";
      document.documentElement.dataset.theme = resolved;
      document.documentElement.style.colorScheme = resolved;
    };
    applyTheme();
    if (settings.theme !== "system") return;
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [i18n, settings]);

  return null;
}

export function GlobalActivityIndicator() {
  const fetching = useIsFetching();
  const mutating = useIsMutating();
  const { t } = useTranslation();
  const active = fetching + mutating > 0;
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    if (!active) {
      setVisible(false);
      return;
    }
    const timer = window.setTimeout(() => setVisible(true), 180);
    return () => window.clearTimeout(timer);
  }, [active]);

  return visible ? (
    <div
      className="global-activity"
      role="progressbar"
      aria-label={t("common.working")}
      aria-valuetext={t("common.working")}
    >
      <span />
    </div>
  ) : null;
}

const onboardingSteps = [
  { icon: ShieldCheck, title: "onboarding.securityTitle", body: "onboarding.securityBody" },
  { icon: FolderCheck, title: "onboarding.setupTitle", body: "onboarding.setupBody" },
  { icon: Layers3, title: "onboarding.profileTitle", body: "onboarding.profileBody" },
] as const;

export function OnboardingDialog() {
  const bootstrap = useAppBootstrap();
  const updateSettings = useUpdateAppSettings();
  const navigate = useNavigate();
  const { t } = useTranslation();
  const [dismissed, setDismissed] = useState(false);
  const [step, setStep] = useState(0);
  const closeButton = useRef<HTMLButtonElement>(null);
  const settings = bootstrap.data?.settings;
  const request = useOnboarding();
  const open = Boolean(settings && (request.requested || (!settings.onboardingCompleted && !dismissed)));
  const current = onboardingSteps[step] ?? onboardingSteps[0];
  const Icon = current.icon;

  const dismiss = () => {
    if (updateSettings.isPending) return;
    setDismissed(true);
    request.close();
  };
  const dialog = useDialogFocus<HTMLElement>(open, dismiss, closeButton);

  if (!open || !settings) return null;

  const complete = async () => {
    try {
      await updateSettings.mutateAsync({ ...settings, onboardingCompleted: true });
      request.close();
    } catch {
      // Keep the dialog open so its error remains actionable.
    }
  };

  return (
    <div className="modal-backdrop onboarding-backdrop" role="presentation">
      <section
        ref={dialog}
        className="onboarding-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="onboarding-title"
      >
        <button
          ref={closeButton}
          className="icon-button onboarding-dialog__close"
          type="button"
          aria-label={t("common.close")}
          disabled={updateSettings.isPending}
          onClick={dismiss}
        >
          <X size={17} />
        </button>
        <div className="onboarding-dialog__intro">
          <span className="eyebrow">{t("onboarding.eyebrow")}</span>
          <h2 id="onboarding-title">{t("onboarding.title")}</h2>
          <p>{t("onboarding.intro")}</p>
        </div>
        <div className="onboarding-step">
          <div className="onboarding-step__icon">
            <Icon size={28} />
          </div>
          <span>{t("onboarding.step", { current: step + 1, total: onboardingSteps.length })}</span>
          <h3>{t(current.title)}</h3>
          <p>{t(current.body)}</p>
        </div>
        <div className="onboarding-dots" aria-hidden="true">
          {onboardingSteps.map((item, index) => (
            <span className={index === step ? "is-active" : ""} key={item.title} />
          ))}
        </div>
        {updateSettings.isError ? (
          <p className="inline-error">{commandErrorMessage(updateSettings.error)}</p>
        ) : null}
        <div className="onboarding-dialog__actions">
          <button
            className="button button--ghost"
            type="button"
            disabled={updateSettings.isPending}
            onClick={dismiss}
          >
            {t("common.later")}
          </button>
          {step === 1 ? (
            <button
              className="button button--secondary"
              type="button"
              onClick={() => {
                dismiss();
                navigate("/settings");
              }}
            >
              {t("onboarding.openSettings")}
            </button>
          ) : null}
          <button
            className="button button--primary"
            type="button"
            disabled={updateSettings.isPending}
            onClick={() => {
              if (step < onboardingSteps.length - 1) setStep((value) => value + 1);
              else void complete();
            }}
          >
            {step === onboardingSteps.length - 1 ? t("common.finish") : t("common.next")}
          </button>
        </div>
      </section>
    </div>
  );
}
