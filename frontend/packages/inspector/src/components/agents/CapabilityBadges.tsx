import type { ComponentType } from "react";
import {
  Activity,
  AlertTriangle,
  Brain,
  Download,
  FileDiff,
  GitBranch,
  HelpCircle,
  Image,
  MessageSquare,
  Paperclip,
  PlayCircle,
  Plug,
  Shield,
  Terminal,
  Wrench
} from "lucide-react";
import type { AgentCapabilitiesView } from "../../types/agents";

const badges = [
  { key: "planMode", label: "Plan", icon: GitBranch },
  { key: "permissions", label: "Perms", icon: Shield },
  { key: "questions", label: "Q&A", icon: HelpCircle },
  { key: "toolCalls", label: "Tool Calls", icon: Wrench },
  { key: "toolResults", label: "Tool Results", icon: Download },
  { key: "textMessages", label: "Text", icon: MessageSquare },
  { key: "images", label: "Images", icon: Image },
  { key: "fileAttachments", label: "Files", icon: Paperclip },
  { key: "sessionLifecycle", label: "Lifecycle", icon: PlayCircle },
  { key: "errorEvents", label: "Errors", icon: AlertTriangle },
  { key: "reasoning", label: "Reasoning", icon: Brain },
  { key: "commandExecution", label: "Commands", icon: Terminal },
  { key: "fileChanges", label: "File Changes", icon: FileDiff },
  { key: "mcpTools", label: "MCP", icon: Plug },
  { key: "streamingDeltas", label: "Deltas", icon: Activity }
] as const;

type BadgeItem = (typeof badges)[number];

const getEnabled = (capabilities: AgentCapabilitiesView, key: BadgeItem["key"]) =>
  Boolean((capabilities as Record<string, boolean | undefined>)[key]);

const CapabilityBadges = ({ capabilities }: { capabilities: AgentCapabilitiesView }) => {
  return (
    <div className="capability-badges">
      {badges.map(({ key, label, icon: Icon }) => (
        <span key={key} className={`capability-badge ${getEnabled(capabilities, key) ? "enabled" : "disabled"}`}>
          <Icon size={12} />
          <span>{label}</span>
        </span>
      ))}
    </div>
  );
};

export default CapabilityBadges;
