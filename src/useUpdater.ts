import { useCallback, useEffect, useRef, useState } from "react";
import { api, errText } from "./api";
import type { UpdateInfo, UpdateState } from "./types";

export type Phase =
  | "idle" | "checking" | "available" | "installing" | "ready" | "uptodate" | "rolledback" | "error";

export function useUpdater(autoCheck: boolean) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [state, setState] = useState<UpdateState | null>(null);
  const phaseRef = useRef<Phase>("idle");
  const manualRef = useRef(false);
  const dismissed = useRef<string | null>(null);

  const set = useCallback((p: Phase) => {
    phaseRef.current = p;
    setPhase(p);
  }, []);

  const refreshState = useCallback(() => {
    api.updateState().then(setState).catch(() => setState(null));
  }, []);

  const check = useCallback(async (manual: boolean) => {
    if (["checking", "installing", "ready"].includes(phaseRef.current)) return;
    manualRef.current = manual;
    set("checking");
    setError(null);
    try {
      const i = await api.checkUpdate();
      setInfo(i);
      if (i.available) set(manual || dismissed.current !== i.latest ? "available" : "idle");
      else set(manual ? "uptodate" : "idle");
    } catch (e) {
      if (manual) { setError(errText(e)); set("error"); } else set("idle");
    }
  }, [set]);

  useEffect(() => { refreshState(); }, [refreshState]);

  useEffect(() => {
    if (!autoCheck) return;
    const first = setTimeout(() => { void check(false); }, 8000);
    const every = setInterval(() => { void check(false); }, 6 * 3600 * 1000);
    return () => { clearTimeout(first); clearInterval(every); };
  }, [autoCheck, check]);

  const install = useCallback(async () => {
    set("installing");
    try {
      await api.installUpdate();
      set("ready");
      refreshState();
    } catch (e) {
      setError(errText(e));
      set("error");
    }
  }, [set, refreshState]);

  const rollback = useCallback(async () => {
    try {
      await api.rollbackUpdate();
      set("rolledback");
      refreshState();
    } catch (e) {
      setError(errText(e));
      set("error");
    }
  }, [set, refreshState]);

  const restart = useCallback(() => {
    api.restartApp().catch((e) => { setError(errText(e)); set("error"); });
  }, [set]);

  const dismiss = useCallback(() => {
    dismissed.current = info?.latest ?? null;
    set("idle");
  }, [info, set]);

  const visible = phase !== "idle" && (phase !== "checking" || manualRef.current);
  return { phase, info, error, state, visible, check, install, rollback, restart, dismiss };
}

export type Updater = ReturnType<typeof useUpdater>;
