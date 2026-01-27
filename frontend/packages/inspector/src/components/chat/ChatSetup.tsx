import { PauseCircle, PlayCircle } from "lucide-react";
import type { AgentModeInfo } from "sandbox-agent";

const ChatSetup = ({
  agentId,
  agentMode,
  permissionMode,
  model,
  variant,
  streamMode,
  polling,
  availableAgents,
  activeModes,
  currentAgentVersion,
  onAgentChange,
  onAgentModeChange,
  onPermissionModeChange,
  onModelChange,
  onVariantChange,
  onStreamModeChange,
  onToggleStream
}: {
  agentId: string;
  agentMode: string;
  permissionMode: string;
  model: string;
  variant: string;
  streamMode: "poll" | "sse";
  polling: boolean;
  availableAgents: string[];
  activeModes: AgentModeInfo[];
  currentAgentVersion?: string | null;
  onAgentChange: (value: string) => void;
  onAgentModeChange: (value: string) => void;
  onPermissionModeChange: (value: string) => void;
  onModelChange: (value: string) => void;
  onVariantChange: (value: string) => void;
  onStreamModeChange: (value: "poll" | "sse") => void;
  onToggleStream: () => void;
}) => {
  return (
    <div className="setup-row">
      <select className="setup-select" value={agentId} onChange={(e) => onAgentChange(e.target.value)} title="Agent">
        {availableAgents.map((id) => (
          <option key={id} value={id}>
            {id}
          </option>
        ))}
      </select>

      <select
        className="setup-select"
        value={agentMode}
        onChange={(e) => onAgentModeChange(e.target.value)}
        title="Mode"
      >
        {activeModes.length > 0 ? (
          activeModes.map((mode) => (
            <option key={mode.id} value={mode.id}>
              {mode.name || mode.id}
            </option>
          ))
        ) : (
          <option value="">Mode</option>
        )}
      </select>

      <select
        className="setup-select"
        value={permissionMode}
        onChange={(e) => onPermissionModeChange(e.target.value)}
        title="Permission Mode"
      >
        <option value="default">Default</option>
        <option value="plan">Plan</option>
        <option value="bypass">Bypass</option>
      </select>

      <input
        className="setup-input"
        value={model}
        onChange={(e) => onModelChange(e.target.value)}
        placeholder="Model"
        title="Model"
      />

      <input
        className="setup-input"
        value={variant}
        onChange={(e) => onVariantChange(e.target.value)}
        placeholder="Variant"
        title="Variant"
      />

      <div className="setup-stream">
        <select
          className="setup-select-small"
          value={streamMode}
          onChange={(e) => onStreamModeChange(e.target.value as "poll" | "sse")}
          title="Stream Mode"
        >
          <option value="poll">Poll</option>
          <option value="sse">SSE</option>
        </select>
        <button
          className={`setup-stream-btn ${polling ? "active" : ""}`}
          onClick={onToggleStream}
          title={polling ? "Stop streaming" : "Start streaming"}
        >
          {polling ? (
            <>
              <PauseCircle size={14} />
              <span>Pause</span>
            </>
          ) : (
            <>
              <PlayCircle size={14} />
              <span>Resume</span>
            </>
          )}
        </button>
      </div>

      {currentAgentVersion && (
        <span className="setup-version" title="Installed version">
          v{currentAgentVersion}
        </span>
      )}
    </div>
  );
};

export default ChatSetup;
