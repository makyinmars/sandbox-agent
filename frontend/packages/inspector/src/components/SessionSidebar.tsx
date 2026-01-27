import { Plus, RefreshCw } from "lucide-react";
import type { SessionInfo } from "sandbox-agent";

const SessionSidebar = ({
  sessions,
  selectedSessionId,
  onSelectSession,
  onRefresh,
  onCreateSession
}: {
  sessions: SessionInfo[];
  selectedSessionId: string;
  onSelectSession: (session: SessionInfo) => void;
  onRefresh: () => void;
  onCreateSession: () => void;
}) => {
  return (
    <div className="session-sidebar">
      <div className="sidebar-header">
        <span className="sidebar-title">Sessions</span>
        <div className="sidebar-header-actions">
          <button className="sidebar-icon-btn" onClick={onRefresh} title="Refresh sessions">
            <RefreshCw size={14} />
          </button>
          <button className="sidebar-add-btn" onClick={onCreateSession} title="New session">
            <Plus size={14} />
          </button>
        </div>
      </div>

      <div className="session-list">
        {sessions.length === 0 ? (
          <div className="sidebar-empty">No sessions yet.</div>
        ) : (
          sessions.map((session) => (
            <button
              key={session.sessionId}
              className={`session-item ${session.sessionId === selectedSessionId ? "active" : ""}`}
              onClick={() => onSelectSession(session)}
            >
              <div className="session-item-id">{session.sessionId}</div>
              <div className="session-item-meta">
                <span className="session-item-agent">{session.agent}</span>
                <span className="session-item-events">{session.eventCount} events</span>
                {session.ended && <span className="session-item-ended">ended</span>}
              </div>
            </button>
          ))
        )}
      </div>
    </div>
  );
};

export default SessionSidebar;
