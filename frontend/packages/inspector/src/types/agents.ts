import type { AgentCapabilities } from "sandbox-agent";

export type AgentCapabilitiesView = AgentCapabilities & {
  toolResults?: boolean;
  textMessages?: boolean;
  images?: boolean;
  fileAttachments?: boolean;
  sessionLifecycle?: boolean;
  errorEvents?: boolean;
  reasoning?: boolean;
  commandExecution?: boolean;
  fileChanges?: boolean;
  mcpTools?: boolean;
  streamingDeltas?: boolean;
};

export const emptyCapabilities: AgentCapabilitiesView = {
  planMode: false,
  permissions: false,
  questions: false,
  toolCalls: false,
  toolResults: false,
  textMessages: false,
  images: false,
  fileAttachments: false,
  sessionLifecycle: false,
  errorEvents: false,
  reasoning: false,
  commandExecution: false,
  fileChanges: false,
  mcpTools: false,
  streamingDeltas: false
};
