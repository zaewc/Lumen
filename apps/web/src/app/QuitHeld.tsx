import { useEffect, useState } from "react";
import { followQuitHeld, useMoving } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { Notice } from "@/shared/ui";

/**
 * Says why closing Lumen waited: a clean-up or Quarantine action is moving
 * items (UX-3). Gone once it finishes; Lumen does not close by itself.
 */
export function QuitHeld() {
  const { m } = useI18n();
  const moving = useMoving();
  const [held, setHeld] = useState(false);
  useEffect(() => {
    if (!moving) {
      setHeld(false);
      return undefined;
    }
    return followQuitHeld(window, () => setHeld(true));
  }, [moving]);
  if (!held || !moving) {
    return null;
  }
  return (
    <div role="status" className="shrink-0 border-b border-line px-5 py-3 min-[960px]:px-8">
      <Notice tone="warning">{m.workspace.quitHeld}</Notice>
    </div>
  );
}
