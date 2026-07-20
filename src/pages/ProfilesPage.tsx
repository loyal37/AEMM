import {
  ArrowRightLeft,
  Check,
  Clock3,
  Copy,
  Layers3,
  ListOrdered,
  LoaderCircle,
  PackageCheck,
  Pencil,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import { useState, type FormEvent } from "react";
import { EmptyState } from "../components/ui/EmptyState";
import { PageHeader } from "../components/ui/PageHeader";
import { LoadOrderDialog } from "../features/profiles/LoadOrderDialog";
import { formatTimestamp } from "../features/mods/modQuery";
import {
  useCopyProfile,
  useCreateProfile,
  useDeleteProfile,
  useProfiles,
  useReorderProfileMods,
  useRenameProfile,
  useSwitchProfile,
} from "../features/profiles/useProfiles";
import { commandErrorMessage } from "../lib/tauri";
import type { Profile } from "../types/app";

type EditorState =
  | { mode: "create"; profile: null }
  | { mode: "rename" | "copy"; profile: Profile };

export function ProfilesPage() {
  const profiles = useProfiles();
  const create = useCreateProfile();
  const rename = useRenameProfile();
  const copy = useCopyProfile();
  const remove = useDeleteProfile();
  const switchProfile = useSwitchProfile();
  const reorder = useReorderProfileMods();
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [orderProfile, setOrderProfile] = useState<Profile | null>(null);
  const [name, setName] = useState("");
  const items = profiles.data ?? [];
  const active = items.find((profile) => profile.isActive);
  const mutationPending =
    create.isPending ||
    rename.isPending ||
    copy.isPending ||
    remove.isPending ||
    switchProfile.isPending ||
    reorder.isPending;

  const openEditor = (state: EditorState) => {
    create.reset();
    rename.reset();
    copy.reset();
    setEditor(state);
    setName(
      state.mode === "create"
        ? ""
        : state.mode === "copy"
          ? `${state.profile.name} 副本`
          : state.profile.name,
    );
  };

  const submitEditor = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!editor) return;
    try {
      if (editor.mode === "create") {
        await create.mutateAsync(name);
      } else if (editor.mode === "rename") {
        await rename.mutateAsync({ profileId: editor.profile.id, name });
      } else {
        await copy.mutateAsync({ sourceProfileId: editor.profile.id, name });
      }
      setEditor(null);
    } catch {
      // Mutation state renders the actionable backend error in the dialog.
    }
  };

  const deleteProfile = async (profile: Profile) => {
    const confirmed = window.confirm(
      `确定删除 Profile“${profile.name}”吗？EFMI Mods 中的模组文件不会被删除。`,
    );
    if (!confirmed) return;
    try {
      await remove.mutateAsync(profile.id);
    } catch {
      // Mutation state renders the error below the page header.
    }
  };

  const editorError = create.error ?? rename.error ?? copy.error;
  const pageError = profiles.error ?? remove.error ?? switchProfile.error;

  return (
    <div className="page-stack profile-page">
      <PageHeader
        eyebrow="配置方案"
        title="Profiles"
        description="保存彼此独立的启用组合和 AEMM 加载顺序；切换失败时会恢复原 Profile 的部署。"
        actions={
          <button
            className="button button--primary"
            type="button"
            disabled={mutationPending}
            onClick={() => openEditor({ mode: "create", profile: null })}
          >
            <Plus size={17} />
            新建配置
          </button>
        }
      />

      {pageError ? <p className="inline-error">{commandErrorMessage(pageError)}</p> : null}
      {switchProfile.data ? (
        <div className="profile-switch-result" role="status">
          <Check size={17} />
          <div>
            <strong>已切换到 {switchProfile.data.profile.name}</strong>
            <span>
              撤销 {switchProfile.data.disabledMods} 个，部署 {switchProfile.data.enabledMods} 个模组。
              {switchProfile.data.guidance ? ` ${switchProfile.data.guidance}` : ""}
            </span>
            {switchProfile.data.warnings.map((warning) => (
              <small key={warning}>{warning}</small>
            ))}
          </div>
        </div>
      ) : null}

      {profiles.isPending ? (
        <section className="panel panel--fill">
          <EmptyState
            icon={LoaderCircle}
            title="正在读取 Profile"
            description="从 SQLite 加载配置方案和模组顺序。"
          />
        </section>
      ) : items.length === 0 ? (
        <section className="panel panel--fill">
          <EmptyState
            icon={Layers3}
            title="还没有 Profile"
            description="创建一个配置方案开始组织模组组合。"
          />
        </section>
      ) : (
        <section className="profile-grid" aria-label="Profile 列表">
          {items.map((profile) => {
            const enabled = profile.mods.filter((item) => item.enabled);
            return (
              <article
                className={`profile-card${profile.isActive ? " is-active" : ""}`}
                key={profile.id}
              >
                <div className="profile-card__header">
                  <div className="profile-card__icon" aria-hidden="true">
                    <Layers3 size={21} />
                  </div>
                  <div>
                    <span className="eyebrow">
                      {profile.isActive ? "当前配置" : "配置方案"}
                    </span>
                    <h2>{profile.name}</h2>
                  </div>
                  {profile.isActive ? (
                    <span className="profile-active-badge">
                      <Check size={12} /> 活动
                    </span>
                  ) : null}
                </div>

                <dl className="profile-card__facts">
                  <div>
                    <dt>
                      <PackageCheck size={14} /> 已启用
                    </dt>
                    <dd>{enabled.length}</dd>
                  </div>
                  <div>
                    <dt>
                      <Clock3 size={14} /> 更新
                    </dt>
                    <dd>{formatTimestamp(profile.updatedAt)}</dd>
                  </div>
                </dl>

                <div className="profile-load-order">
                  <div>
                    <span className="eyebrow">AEMM 加载顺序</span>
                    <small>仅表示保存顺序，不宣称 EFMI 实际胜出优先级</small>
                  </div>
                  {enabled.length ? (
                    <ol>
                      {enabled.slice(0, 5).map((item) => (
                        <li key={item.modId}>
                          <span>{item.loadOrder + 1}</span>
                          <strong>{item.modName}</strong>
                        </li>
                      ))}
                    </ol>
                  ) : (
                    <p>此 Profile 暂未启用模组。</p>
                  )}
                  {enabled.length > 5 ? <small>另有 {enabled.length - 5} 个模组</small> : null}
                </div>

                <div className="profile-card__actions">
                  <button
                    className="button button--primary"
                    type="button"
                    disabled={profile.isActive || mutationPending}
                    onClick={() => switchProfile.mutate(profile.id)}
                  >
                    {switchProfile.isPending && switchProfile.variables === profile.id ? (
                      <LoaderCircle className="spin" size={15} />
                    ) : (
                      <ArrowRightLeft size={15} />
                    )}
                    {profile.isActive ? "正在使用" : "切换"}
                  </button>
                  <button
                    className="icon-button profile-action"
                    type="button"
                    aria-label={`调整 ${profile.name} 的加载顺序`}
                    title={enabled.length > 1 ? "调整加载顺序" : "至少需要两个启用模组"}
                    disabled={enabled.length < 2 || mutationPending}
                    onClick={() => {
                      reorder.reset();
                      setOrderProfile(profile);
                    }}
                  >
                    <ListOrdered size={15} />
                  </button>
                  <button
                    className="icon-button profile-action"
                    type="button"
                    aria-label={`复制 ${profile.name}`}
                    title="复制 Profile"
                    disabled={mutationPending}
                    onClick={() => openEditor({ mode: "copy", profile })}
                  >
                    <Copy size={15} />
                  </button>
                  <button
                    className="icon-button profile-action"
                    type="button"
                    aria-label={`重命名 ${profile.name}`}
                    title="重命名 Profile"
                    disabled={mutationPending}
                    onClick={() => openEditor({ mode: "rename", profile })}
                  >
                    <Pencil size={15} />
                  </button>
                  <button
                    className="icon-button profile-action profile-action--danger"
                    type="button"
                    aria-label={`删除 ${profile.name}`}
                    title={profile.isActive ? "活动 Profile 不能删除" : "删除 Profile"}
                    disabled={profile.isActive || mutationPending}
                    onClick={() => void deleteProfile(profile)}
                  >
                    <Trash2 size={15} />
                  </button>
                </div>
              </article>
            );
          })}
        </section>
      )}

      {editor ? (
        <div className="modal-backdrop" role="presentation">
          <form
            className="profile-editor-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="profile-editor-title"
            onSubmit={(event) => void submitEditor(event)}
          >
            <div className="profile-editor-dialog__header">
              <div>
                <span className="eyebrow">Profile</span>
                <h2 id="profile-editor-title">
                  {editor.mode === "create"
                    ? "新建配置"
                    : editor.mode === "copy"
                      ? "复制配置"
                      : "重命名配置"}
                </h2>
              </div>
              <button
                className="icon-button"
                type="button"
                aria-label="关闭"
                disabled={create.isPending || rename.isPending || copy.isPending}
                onClick={() => setEditor(null)}
              >
                <X size={17} />
              </button>
            </div>
            <label className="profile-name-field">
              <span>Profile 名称</span>
              <input
                autoFocus
                maxLength={64}
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder="例如：角色模组、截图配置"
              />
            </label>
            {editor.mode === "copy" ? (
              <p className="profile-editor-note">
                将复制“{editor.profile.name}”保存的启用状态和加载顺序，不会复制模组文件。
              </p>
            ) : null}
            {editorError ? (
              <p className="inline-error">{commandErrorMessage(editorError)}</p>
            ) : null}
            <div className="profile-editor-dialog__actions">
              <button
                className="button button--secondary"
                type="button"
                disabled={create.isPending || rename.isPending || copy.isPending}
                onClick={() => setEditor(null)}
              >
                取消
              </button>
              <button
                className="button button--primary"
                type="submit"
                disabled={!name.trim() || create.isPending || rename.isPending || copy.isPending}
              >
                {create.isPending || rename.isPending || copy.isPending ? (
                  <LoaderCircle className="spin" size={15} />
                ) : editor.mode === "copy" ? (
                  <Copy size={15} />
                ) : (
                  <Check size={15} />
                )}
                保存
              </button>
            </div>
          </form>
        </div>
      ) : null}

      {orderProfile ? (
        <LoadOrderDialog
          profile={orderProfile}
          pending={reorder.isPending}
          error={reorder.error}
          onClose={() => setOrderProfile(null)}
          onSave={async (modIds) => {
            try {
              await reorder.mutateAsync({ profileId: orderProfile.id, modIds });
              setOrderProfile(null);
            } catch {
              // The dialog renders the backend validation error.
            }
          }}
        />
      ) : null}

      {active ? (
        <p className="profile-page__footnote">
          当前活动 Profile：<strong>{active.name}</strong>。切换会在 EFMI Mods 内原地同步 DISABLED 状态；不会复制模组文件。
        </p>
      ) : null}
    </div>
  );
}
