import { useEffect, useState } from "react";
import { type AiSettings, getAiSettings, setAiSettings, testAiConnection } from "../lib/api";

interface Props {
  open: boolean;
  onClose: () => void;
  /** 保存成功后回调，让 App 立刻刷新 `aiEnabled`，不必重开面板。 */
  onSaved: (settings: AiSettings) => void;
}

/**
 * AI 设置面板（TASK-013）。
 *
 * endpoint 的合法范围由 Rust 侧的 `summarizer.rs` 决定，这里只负责把
 * 拒绝理由原样显示出来 —— 前端不复制一份校验规则，否则两处规则迟早会分叉。
 *
 * 与归档原稿的两处结构差异（都是为了让面板真的可用）：
 * - 遮罩从 `<div onClick>` 改为 `<button className="modal-scrim">`：
 *   非交互元素挂 onClick 通不过 a11y 检查，而无障碍上它本来就是一个按钮。
 * - 「测试连接」只测不存，不会把用户正在编辑的值写盘。
 */
export function AiSettingsPanel({ open, onClose, onSaved }: Props) {
  const [settings, setSettings] = useState<AiSettings | null>(null);
  const [models, setModels] = useState<string[] | null>(null);
  const [busy, setBusy] = useState<"test" | "save" | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setModels(null);
    setMessage(null);
    getAiSettings()
      .then(setSettings)
      .catch((e) => setMessage(`读取设置失败：${String(e)}`));
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  async function save() {
    if (!settings) return;
    setBusy("save");
    setMessage(null);
    try {
      await setAiSettings(settings);
      onSaved(settings);
      setMessage("已保存。");
    } catch (e) {
      setMessage(`保存失败：${String(e)}`);
    } finally {
      setBusy(null);
    }
  }

  async function test() {
    if (!settings) return;
    setBusy("test");
    setMessage(null);
    try {
      const found = await testAiConnection(settings);
      setModels(found);
      setMessage(
        found.length > 0
          ? `连接成功，检测到 ${found.length} 个模型。`
          : "连接成功，但没有可用模型。",
      );
    } catch (e) {
      setModels(null);
      setMessage(`连接失败：${String(e)}`);
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="modal-layer">
      <button type="button" className="modal-scrim" aria-label="关闭 AI 设置" onClick={onClose} />
      <div className="modal" role="dialog" aria-modal="true" aria-labelledby="ai-settings-title">
        <div className="modal-header">
          <h2 id="ai-settings-title">AI 分析设置</h2>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="关闭">
            ✕
          </button>
        </div>

        <div className="modal-body">
          {!settings && <div className="empty">读取中…</div>}

          {settings && (
            <>
              <label className="field checkbox">
                <input
                  type="checkbox"
                  checked={settings.enabled}
                  onChange={(e) => setSettings({ ...settings, enabled: e.target.checked })}
                />
                <span>启用本地 AI 摘要</span>
              </label>

              <p className="hint">
                只允许本机地址（localhost / 127.0.0.1 / [::1]）。RepoPrism 只会把
                <strong>文件的相对路径与规则结果</strong>
                发给你自己的模型，不会发送仓库路径、remote 或 diff 正文，也不会发往任何云端服务。
              </p>

              <label className="field">
                <span>Endpoint</span>
                <input
                  value={settings.endpoint}
                  onChange={(e) => setSettings({ ...settings, endpoint: e.target.value })}
                  placeholder="http://localhost:11434"
                  spellCheck={false}
                />
              </label>

              <label className="field">
                <span>Model</span>
                <input
                  value={settings.model}
                  onChange={(e) => setSettings({ ...settings, model: e.target.value })}
                  placeholder="llama3.2"
                  spellCheck={false}
                  list="ollama-models"
                />
                {models && models.length > 0 && (
                  <datalist id="ollama-models">
                    {models.map((name) => (
                      <option key={name} value={name} />
                    ))}
                  </datalist>
                )}
              </label>

              {message && <div className="modal-message">{message}</div>}

              <div className="modal-actions">
                <button type="button" className="ghost" onClick={test} disabled={busy !== null}>
                  {busy === "test" ? "测试中…" : "测试连接"}
                </button>
                <button type="button" className="primary" onClick={save} disabled={busy !== null}>
                  {busy === "save" ? "保存中…" : "保存"}
                </button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
