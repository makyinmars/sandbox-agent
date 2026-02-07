import type { AgentModeInfo } from "sandbox-agent";

const ChatSetup = ({
  agentMode,
  permissionMode,
  model,
  variant,
  activeModes,
  hasSession,
  modelDisabled,
  variantDisabled,
  modelHint,
  variantHint,
  modesLoading,
  modesError,
  onAgentModeChange,
  onPermissionModeChange,
  onModelChange,
  onVariantChange,
  onSessionUpdate
}: {
  agentMode: string;
  permissionMode: string;
  model: string;
  variant: string;
  activeModes: AgentModeInfo[];
  hasSession: boolean;
  modelDisabled?: boolean;
  variantDisabled?: boolean;
  modelHint?: string | null;
  variantHint?: string | null;
  modesLoading: boolean;
  modesError: string | null;
  onAgentModeChange: (value: string) => void;
  onPermissionModeChange: (value: string) => void;
  onModelChange: (value: string) => void;
  onVariantChange: (value: string) => void;
  onSessionUpdate: () => void;
}) => {
  return (
    <div className="setup-row">
      <div className="setup-field">
        <span className="setup-label">Mode</span>
        <select
          className="setup-select"
          value={agentMode}
          onChange={(e) => onAgentModeChange(e.target.value)}
          title="Mode"
          disabled={!hasSession || modesLoading || Boolean(modesError)}
        >
          {modesLoading ? (
            <option value="">Loading modes...</option>
          ) : modesError ? (
            <option value="">{modesError}</option>
          ) : activeModes.length > 0 ? (
            activeModes.map((mode) => (
              <option key={mode.id} value={mode.id}>
                {mode.name || mode.id}
              </option>
            ))
          ) : (
            <option value="">Mode</option>
          )}
        </select>
      </div>

      <div className="setup-field">
        <span className="setup-label">Permission</span>
        <select
          className="setup-select"
          value={permissionMode}
          onChange={(e) => onPermissionModeChange(e.target.value)}
          title="Permission Mode"
          disabled={!hasSession}
        >
          <option value="default">Default</option>
          <option value="plan">Plan</option>
          <option value="bypass">Bypass</option>
        </select>
      </div>

      <div className="setup-field">
        <span className="setup-label">Model</span>
        <input
          className="setup-input"
          value={model}
          onChange={(e) => onModelChange(e.target.value)}
          onBlur={onSessionUpdate}
          placeholder="Model"
          title="Model"
          disabled={!hasSession || Boolean(modelDisabled)}
        />
        {modelHint ? <span className="setup-hint">{modelHint}</span> : null}
      </div>

      <div className="setup-field">
        <span className="setup-label">Variant</span>
        <input
          className="setup-input"
          value={variant}
          onChange={(e) => onVariantChange(e.target.value)}
          onBlur={onSessionUpdate}
          placeholder="Variant"
          title="Variant"
          disabled={!hasSession || Boolean(variantDisabled)}
        />
        {variantHint ? <span className="setup-hint">{variantHint}</span> : null}
      </div>
    </div>
  );
};

export default ChatSetup;
