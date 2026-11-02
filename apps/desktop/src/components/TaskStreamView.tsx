interface TaskStreamViewProps {
  reasoning: string;
  content: string;
  active: boolean;
  label?: string;
}

export function TaskStreamView({
  reasoning,
  content,
  active,
  label = "正在生成…",
}: TaskStreamViewProps) {
  if (!reasoning && !content) {
    return <p className="muted">{label}</p>;
  }
  return (
    <div className="task-stream">
      {active && <div className="task-stream-status">{label}</div>}
      {reasoning && (
        <details className="task-stream-reasoning" open>
          <summary>思考过程</summary>
          <pre>{reasoning}</pre>
        </details>
      )}
      {content && (
        <details className="task-stream-content" open>
          <summary>已生成内容</summary>
          <pre>{content}</pre>
        </details>
      )}
    </div>
  );
}
