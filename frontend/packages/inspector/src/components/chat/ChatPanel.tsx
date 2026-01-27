import { MessageSquare, Plus, Terminal } from "lucide-react";
import type { AgentModeInfo, PermissionEventData, QuestionEventData } from "sandbox-agent";
import ApprovalsTab from "../debug/ApprovalsTab";
import ChatInput from "./ChatInput";
import ChatMessages from "./ChatMessages";
import ChatSetup from "./ChatSetup";
import type { TimelineEntry } from "./types";

const ChatPanel = ({
  sessionId,
  polling,
  transcriptEntries,
  sessionError,
  message,
  onMessageChange,
  onSendMessage,
  onKeyDown,
  onCreateSession,
  messagesEndRef,
  agentId,
  agentMode,
  permissionMode,
  model,
  variant,
  streamMode,
  availableAgents,
  activeModes,
  currentAgentVersion,
  onAgentChange,
  onAgentModeChange,
  onPermissionModeChange,
  onModelChange,
  onVariantChange,
  onStreamModeChange,
  onToggleStream,
  questionRequests,
  permissionRequests,
  questionSelections,
  onSelectQuestionOption,
  onAnswerQuestion,
  onRejectQuestion,
  onReplyPermission
}: {
  sessionId: string;
  polling: boolean;
  transcriptEntries: TimelineEntry[];
  sessionError: string | null;
  message: string;
  onMessageChange: (value: string) => void;
  onSendMessage: () => void;
  onKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  onCreateSession: () => void;
  messagesEndRef: React.RefObject<HTMLDivElement>;
  agentId: string;
  agentMode: string;
  permissionMode: string;
  model: string;
  variant: string;
  streamMode: "poll" | "sse";
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
  questionRequests: QuestionEventData[];
  permissionRequests: PermissionEventData[];
  questionSelections: Record<string, string[][]>;
  onSelectQuestionOption: (requestId: string, optionLabel: string) => void;
  onAnswerQuestion: (request: QuestionEventData) => void;
  onRejectQuestion: (requestId: string) => void;
  onReplyPermission: (requestId: string, reply: "once" | "always" | "reject") => void;
}) => {
  const hasApprovals = questionRequests.length > 0 || permissionRequests.length > 0;

  return (
    <div className="chat-panel">
      <div className="panel-header">
        <div className="panel-header-left">
          <MessageSquare className="button-icon" />
          <span className="panel-title">Session</span>
          {sessionId && <span className="session-id-display">{sessionId}</span>}
        </div>
        {polling && <span className="pill accent">Live</span>}
      </div>

      <div className="messages-container">
        {!sessionId ? (
          <div className="empty-state">
            <MessageSquare className="empty-state-icon" />
            <div className="empty-state-title">No Session Selected</div>
            <p className="empty-state-text">Create a new session to start chatting with an agent.</p>
            <button className="button primary" onClick={onCreateSession}>
              <Plus className="button-icon" />
              Create Session
            </button>
          </div>
        ) : transcriptEntries.length === 0 && !sessionError ? (
          <div className="empty-state">
            <Terminal className="empty-state-icon" />
            <div className="empty-state-title">Ready to Chat</div>
            <p className="empty-state-text">Send a message to start a conversation with the agent.</p>
          </div>
        ) : (
          <ChatMessages
            entries={transcriptEntries}
            sessionError={sessionError}
            messagesEndRef={messagesEndRef}
          />
        )}
      </div>

      {hasApprovals && (
        <div className="approvals-inline">
          <div className="approvals-inline-header">Approvals</div>
          <ApprovalsTab
            questionRequests={questionRequests}
            permissionRequests={permissionRequests}
            questionSelections={questionSelections}
            onSelectQuestionOption={onSelectQuestionOption}
            onAnswerQuestion={onAnswerQuestion}
            onRejectQuestion={onRejectQuestion}
            onReplyPermission={onReplyPermission}
          />
        </div>
      )}

      <ChatInput
        message={message}
        onMessageChange={onMessageChange}
        onSendMessage={onSendMessage}
        onKeyDown={onKeyDown}
        placeholder={sessionId ? "Send a message..." : "Select or create a session first"}
        disabled={!sessionId}
      />

      <ChatSetup
        agentId={agentId}
        agentMode={agentMode}
        permissionMode={permissionMode}
        model={model}
        variant={variant}
        streamMode={streamMode}
        polling={polling}
        availableAgents={availableAgents}
        activeModes={activeModes}
        currentAgentVersion={currentAgentVersion}
        onAgentChange={onAgentChange}
        onAgentModeChange={onAgentModeChange}
        onPermissionModeChange={onPermissionModeChange}
        onModelChange={onModelChange}
        onVariantChange={onVariantChange}
        onStreamModeChange={onStreamModeChange}
        onToggleStream={onToggleStream}
      />
    </div>
  );
};

export default ChatPanel;
