# ADR-0067: Set up the helper and Full Disk Access on first launch

- Status: Accepted (owner decision, 2026-11-16)
- Date: 2026-11-16
- Amends:
  - [ADR-0063](0063-use-administrator-rights-through-an-opt-in-helper.md): "Timing"
    and "Opt-in" (the helper is offered on first launch, with turning it on as the
    main choice);
  - [ADR-0050](0050-ask-about-ai-review-on-first-launch.md): the AI review question is
    the first step of a first-launch setup.

## Context and problem statement

The owner wants Lumen ready to clean up from its first run (2026-11-16): the
administrator helper on by default, and every permission set up the first time the
app opens. Until now:

- the helper was set up only the first time the person chose something that needed
  it (ADR-0063), so a person met it in the middle of a clean-up;
- Full Disk Access was explained only in Settings › Permissions, so a first scan often
  missed protected folders;
- the first launch asked only about AI review (ADR-0050).

An app cannot turn the helper on by itself. `SMAppService.daemon` registration needs
the person to allow Lumen in System Settings › General › Login Items, as an
administrator. Full Disk Access can only be granted by the person in System Settings.
So "on by default" can only mean that Lumen asks at once, with turning on as the main
choice.

## Decision drivers

- Permissions must never make cleaning up awkward (ADR-0063's goal).
- The person hears from Lumen what the helper is before macOS asks for an
  administrator password.
- Every request can still be declined with a visible choice
  ([ux-writing](../design/ux-writing.md)).
- Nothing changes for people who have already set Lumen up.

## Considered options

1. **A first-launch setup: AI review, then the helper, then Full Disk Access.** The
   helper step's main button turns it on.
2. **Register the helper at every launch without asking.** Rejected: macOS shows
   "Background item added" with no word from Lumen about what it is, and the person
   approves an administrator component they were never told about.
3. **Keep setting the helper up when first needed.** Rejected by the owner: the
   approval interrupts the first clean-up that needs it.

## Decision outcome

Chosen option: **1**.

- **When.** While AI review has not been chosen (ADR-0050's "not chosen yet"), the
  first screen is a setup of up to three steps, in this order:
  1. **AI review:** ADR-0050's question, unchanged.
  2. **Administrator helper** (where the host has one, macOS): what it is, what it may
     do (ADR-0063), and that macOS will ask for an administrator's approval. The main
     button turns it on, which registers it and opens the system approval; "Later"
     moves on and leaves it off. While approval is pending, the step offers to open
     Login Items, and it moves on when the person is ready.
  3. **Full Disk Access** (macOS): why Lumen asks, how to grant it, and that Lumen
     must be reopened afterwards. One button opens System Settings at Full Disk
     Access; "Start" finishes the setup either way.
- A step the host cannot offer is skipped. A step's choice applies at once, so quitting
  in the middle keeps what was chosen. The remaining steps are not asked again: both
  stay in Settings › Permissions.
- **Existing users** keep their setup. Someone who already chose about AI review does
  not see the setup.
- ADR-0063's rules on what the helper may do, its signing, its checks and turning it
  off are unchanged. Only when it is offered changes.
- The setup is a page, not a dialog, like ADR-0050's question: it takes no focus, and
  it stays the explicit exception to the launch rule in
  [ux-writing](../design/ux-writing.md).

## Consequences

- Good: a new install can reach a clean-up of other accounts' apps and protected
  folders without stopping for permissions.
- Good: the person hears what the helper is before macOS asks for an administrator.
- Bad: more people run a privileged helper, including some who will never need it.
  ADR-0063's controls bound what it can do, and it runs only on demand.
- Neutral: Windows has no helper step yet; it gains one with the Windows service
  (roadmap 16.10).
