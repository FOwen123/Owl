import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { commands, type Meeting } from "@/bindings";

/** Every meeting, newest first, kept current. `meetings-updated` fires for
 * each live transcript line, so one fetch runs at a time and a burst of
 * events during it becomes one more fetch, not one per event. */
export const useMeetings = () => {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  useEffect(() => {
    let fetching = false;
    let again = false;
    let gone = false;
    const refresh = async () => {
      if (fetching) {
        again = true;
        return;
      }
      fetching = true;
      do {
        again = false;
        const result = await commands.getMeetings();
        if (gone) return;
        if (result.status === "ok") setMeetings(result.data);
      } while (again);
      fetching = false;
    };
    refresh();
    const unlisten = listen("meetings-updated", refresh);
    return () => {
      gone = true;
      unlisten.then((fn) => fn());
    };
  }, []);
  return meetings;
};
