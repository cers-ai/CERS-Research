import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

interface TaskDelta {
  taskId: string;
  kind: "reasoning" | "content";
  text: string;
}

export function useTaskStream(taskId: string | null) {
  const [reasoning, setReasoning] = useState("");
  const [content, setContent] = useState("");
  const [active, setActive] = useState(false);

  useEffect(() => {
    if (!taskId) {
      setReasoning("");
      setContent("");
      setActive(false);
      return;
    }
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const register = async () => {
      const start = await listen<{ taskId: string }>("task-stream-start", (event) => {
        if (!disposed && event.payload.taskId === taskId) {
          setReasoning("");
          setContent("");
          setActive(true);
        }
      });
      const delta = await listen<TaskDelta>("task-delta", (event) => {
        if (disposed || event.payload.taskId !== taskId) {
          return;
        }
        if (event.payload.kind === "reasoning") {
          setReasoning((current) => current + event.payload.text);
        } else {
          setContent((current) => current + event.payload.text);
        }
      });
      const end = await listen<{ taskId: string }>("task-stream-end", (event) => {
        if (!disposed && event.payload.taskId === taskId) {
          setActive(false);
        }
      });
      unlisteners.push(start, delta, end);
    };
    void register();
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, [taskId]);

  return { reasoning, content, active };
}
