import { Send } from "lucide-react";

const ChatInput = ({
  message,
  onMessageChange,
  onSendMessage,
  onKeyDown,
  placeholder,
  disabled
}: {
  message: string;
  onMessageChange: (value: string) => void;
  onSendMessage: () => void;
  onKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => void;
  placeholder: string;
  disabled: boolean;
}) => {
  return (
    <div className="input-container">
      <div className="input-wrapper">
        <textarea
          value={message}
          onChange={(event) => onMessageChange(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder={placeholder}
          rows={1}
          disabled={disabled}
        />
        <button className="send-button" onClick={onSendMessage} disabled={disabled || !message.trim()}>
          <Send />
        </button>
      </div>
    </div>
  );
};

export default ChatInput;
