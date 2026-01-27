import { Clipboard } from "lucide-react";

import type { RequestLog } from "../../types/requestLog";

const RequestLogTab = ({
  requestLog,
  copiedLogId,
  onClear,
  onCopy
}: {
  requestLog: RequestLog[];
  copiedLogId: number | null;
  onClear: () => void;
  onCopy: (entry: RequestLog) => void;
}) => {
  return (
    <>
      <div className="inline-row" style={{ marginBottom: 12, justifyContent: "space-between" }}>
        <span className="card-meta">{requestLog.length} requests</span>
        <button className="button ghost small" onClick={onClear}>
          Clear
        </button>
      </div>

      {requestLog.length === 0 ? (
        <div className="card-meta">No requests logged yet.</div>
      ) : (
        requestLog.map((entry) => (
          <div key={entry.id} className="log-item">
            <span className="log-method">{entry.method}</span>
            <span className="log-url text-truncate">{entry.url}</span>
            <span className={`log-status ${entry.status && entry.status < 400 ? "ok" : "error"}`}>
              {entry.status || "ERR"}
            </span>
            <div className="log-meta">
              <span>
                {entry.time}
                {entry.error && ` - ${entry.error}`}
              </span>
              <button className="copy-button" onClick={() => onCopy(entry)}>
                <Clipboard />
                {copiedLogId === entry.id ? "Copied" : "curl"}
              </button>
            </div>
          </div>
        ))
      )}
    </>
  );
};

export default RequestLogTab;
