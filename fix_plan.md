# Event System Fix Plan

## Current State
Phases 1-6, 8, and 12 of the event system plan (plans/ui-upgrades/3-events.md) are DONE.
The remaining phases are:
- **Phase 7: Text Input and IME** (in progress — Tasks 1-5 done, Task 6 remaining)
- **Phase 9: Drag and Drop** (in progress — Tasks 1-5 done, Tasks 6-9 remaining)
- **Phase 10: Clipboard** (in progress — Tasks 1-4 done, Tasks 5-7 remaining)
- Phase 11: Touch Events

## Phase 7: Text Input and IME

### Goal
Full text editing support with IME composition.

### Tasks

#### Task 1: Add BeforeInput/Input event types to input.rs [DONE]
#### Task 2: Add event handlers to Element [DONE]
#### Task 3: Wire text input through proper event dispatch [DONE]
#### Task 4: Integrate zwp_text_input_v3 protocol [DONE]
#### Task 5: Handle IME composition events [DONE]

#### Task 6: Render preedit text [PENDING]
- Display preedit text with underline styling in text input elements
- Show composition cursor position
- Requires changes to rendering pipeline (display_list/renderer)

### Notes
- **Needs VM build/test** — wayland-protocols can't build on macOS
- delete_surrounding_text is stubbed — needs text input element state cooperation

## Phase 10: Clipboard

### Goal
Copy, cut, paste support.

### Tasks

#### Task 1: Add ClipboardData/ClipboardEvent types to input.rs [DONE]
- `ClipboardData` struct with HashMap<String, Vec<u8>>, set/get/set_text/get_text helpers
- `ClipboardEvent` struct with clipboard_data field

#### Task 2: Add clipboard handler slots on Element [DONE]
- `on_copy`, `on_cut` (mutable ClipboardEvent, returns EventResult)
- `on_paste` (immutable ClipboardEvent, returns EventResult)
- Builder methods for all handlers

#### Task 3: Fire Copy/Cut/Paste events on Ctrl+C/X/V [DONE]
- Intercept Ctrl+C/X/V in handle_key_down
- Dispatch KeyDown first, then fire clipboard event if not default-prevented
- Three-phase propagation (target + bubble) for clipboard events
- Copy/Cut: store resulting clipboard data in EventState
- Paste: read clipboard data, fire event, default behavior inserts text in text inputs

#### Task 4: Default clipboard behavior for text inputs [DONE]
- Paste fires BeforeInput(InsertFromPaste) -> Input through text input pipeline
- Falls back to on_change legacy handler

#### Task 5: Integrate wl_data_device::set_selection for setting clipboard [PENDING]
- Requires Wayland wl_data_device integration (VM build/test)

#### Task 6: Integrate wl_data_device::selection for reading clipboard [PENDING]
- Requires Wayland wl_data_device integration (VM build/test)

#### Task 7: Implement primary selection (zwp_primary_selection_device_v1) [PENDING]
- Middle-click paste, requires VM build/test

### Notes
- ClipboardData/ClipboardEvent exported from lib.rs
- Internal clipboard storage in EventState.clipboard_data (temporary until Wayland integration)
- Wayland tasks (5-7) require VM build/test

## Phase 9: Drag and Drop

### Goal
Internal and external DnD support.

### Tasks

#### Task 1: Add DnD event types to input.rs [DONE]
#### Task 2: Add DnD handler slots on Element [DONE]
#### Task 3: Internal DnD state machine in EventState [DONE]
#### Task 4: DragOver/DragEnter/DragLeave via hit testing during drag [DONE]
#### Task 5: Drop/DragEnd on pointer release [DONE]

#### Task 6: Render drag ghost image during drag [PENDING]
- Display ghost image of dragged element at pointer offset
- Original element opacity reduction
- Requires rendering pipeline changes

#### Task 7: Integrate wl_data_device for external DnD (receiving) [PENDING]
- Translate Wayland data_device events to DragEnter/DragOver/DragLeave/Drop
- Needs VM build/test

#### Task 8: Integrate wl_data_device for external DnD (initiating) [PENDING]
- Create wl_data_source, start_drag, handle source events
- Needs VM build/test

#### Task 9: Tests for DnD lifecycle [PENDING]
- Internal DnD lifecycle test
- Drop acceptance/rejection test

### Notes
- Exports added to lib.rs: DragData, DragEvent, DropEffect
- DnD takes priority over legacy on_drag when element has on_drag_start
- External DnD (Tasks 7-8) requires Wayland and needs VM build/test

## Phase 11: Touch Events

### Goal
Native touch support beyond pointer coercion.

### Tasks (all PENDING)
1. Implement SCTK touch handler (delegate_touch)
2. Coerce primary touch to pointer events (with implicit capture)
3. Fire native TouchStart/Move/End/Cancel events for all touches
4. Handle multi-touch (multiple pointer IDs)
5. Store touch shape/orientation data
6. Tests: single touch, multi-touch, touch cancel
