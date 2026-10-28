# Operations and the lifetime of a view

Verified against the shared desktop/browser UI, 2026-10-28.

The host command, restore/finalize batch and confirmed clean-up outlive the page or
inspector that starts them. Their component-local mutation state did not: reopening
could show idle controls while work continued. A second scan could also replace the
evidence associated with an in-flight clean-up.

Confirmed operation state now lives in the existing application session cache, with
one pending run per operation type. Tool execution and Quarantine keep their latest
output/outcome and progress. Clean-up belongs to the scan session and blocks new
scans until its report is recorded. Update installation uses the same session
lifetime. No operation is started merely by subscribing to its state.

An unconfirmed plan remains local and is discarded when its page disappears. AI
setting mutations share pending/error status, with successful changes written to the
existing settings query. Typed keys are not copied into session-operation payloads.
This is in-memory navigation continuity, not a durable job service or a promise
that work survives app shutdown.

Browser verification covers inspector close/reopen during a fixture command,
single/batch restore and finalize across browser Back, scan continuity, confirmed
clean-up across Back with rescan controls locked, discarded unconfirmed preparation,
and pending AI enable/disable writes after returning to Settings. Destructive
browser actions use only the in-memory transport.

Native dialog tests must use trusted mouse/keyboard input when checking cancellable
Escape behavior: a DOM `click()` does not provide the browser's history-action user
activation. Repeated platform close requests can override cancellation by design;
work must therefore remain independent of the dialog even when it cannot prevent
closing. See the [HTML close-watcher rules](https://html.spec.whatwg.org/multipage/interaction.html#close-requests-and-close-watchers).
