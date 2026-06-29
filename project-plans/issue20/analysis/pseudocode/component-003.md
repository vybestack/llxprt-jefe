# Component 003 Pseudocode — Key Routing, Inline Mutation, Agent Chooser (PR Mode)

Plan ID: `PLAN-20260624-PR-MODE`

Requirements: REQ-PR-001,002,003,004,008,010,011,012,013

This component is the key-routing layer (`src/app_input/prs.rs`, `prs_filter.rs`,
`prs_list_dispatch.rs`, `prs_mutation.rs`, plus the `p`/`P` hook in `normal.rs`). Every handler
returns `Option<AppEvent>` (or `KeyHandling`) — it performs NO state mutation and NO I/O. State
transitions go through the reducer; off-thread `gh` calls are spawned by the dispatch layer.

## Pane-cycle vs detail-subfocus binding scheme (Tab cycles ALL panes — issue #46)

Issue #46 explicitly requires: "Same focus cycling (Tab between repo list / PR list / PR detail)."
PR Mode therefore reserves `Tab`/`Shift+Tab` for inter-pane cycling in EVERY pane, and uses `j`/`k`
for detail subfocus traversal. This DIVERGES from Issues mode (which consumes `Tab`/`BackTab` for
subfocus inside its issue-detail pane, `resolve_issue_detail_key_event` `src/app_input/issues.rs`
L149-150); the divergence is intentional and required by #46.

- `Tab`/`Shift+Tab` cycle PANES (repo_list ↔ pr_list ↔ pr_detail) in EVERY pane INCLUDING `PrDetail`.
  In `RepoList`/`PrList` this is achieved by the focus-domain handlers returning `None` for
  `Tab`/`Shift+Tab` so the key falls through to the P7 pane-cycle fallback
  (`resolve_pane_cycle_key_event`, `src/app_input/issues.rs` L183-189). In `PrDetail` the detail
  handler ALSO returns `None` for `Tab`/`Shift+Tab` (it does NOT consume them for subfocus), so they
  likewise fall through to the SAME P7 pane-cycle fallback. The net effect is that `Tab` advances
  `RepoList -> PrList -> PrDetail -> RepoList` and `Shift+Tab` reverses, from any pane.
- Detail SUBFOCUS traversal is bound to `j`/`k` INSIDE the `PrDetail` focus-domain handler:
  `j -> PrDetailSubfocusNext`, `k -> PrDetailSubfocusPrev` (walking body → reviews → checks →
  comments → new-comment → body, skipping empty sections). `j`/`k` are chosen because they are
  vim-style next/prev consistent with list navigation, are currently UNUSED in `src/app_input/`
  (only `Ctrl-k` is bound — `issues.rs` L386 — and `Ctrl-k` is suppressed in PR mode), and do NOT
  collide with the `Up/Down` detail SCROLL binding.
- The `Left`/`Right` ARROWS cycle panes WITHIN the list focus-domain handlers, as Issues mode does:
  - `PrList`: `Left -> PrCycleFocusReverse`, `Right -> PrCycleFocus`
    (mirror `resolve_issue_list_key_event` L124-125).
  - `PrDetail`: `Left -> PrCycleFocusReverse` is OPTIONAL parity (a reverse pane-cycle), NOT the sole
    escape — `Tab`/`Shift+Tab` already cycle out of detail. `Right` is unbound in detail.
  - `RepoList`: `Right -> PrCycleFocus` (mirror `resolve_repo_list_key_event` L178); `Left` unbound.
- Net effect (satisfies issue #46 "Tab between repo list / PR list / PR detail" AND REQ-PR-003):
  from EVERY pane, `Tab`/`Shift+Tab` advance/reverse the three-pane cycle; detail subfocus is on
  `j`/`k`. `Tab` and subfocus never contend, so there is NO binding conflict.

```text
01: FUNCTION resolve_mode_key(key, screen_mode)        // in normal.rs (entry hook)
02:   IF key in {'p','P'} AND screen_mode == Dashboard -> RETURN Handled(Some(EnterPrsMode))
03:   // existing 'i'/'I' (issues) and 's'/'S' (split) arms remain unchanged
04:   RETURN Unhandled

05: FUNCTION handle_dashboard_prs_key(snapshot, key)    // in normal.rs, mirrors issues variant
06:   IF screen_mode != DashboardPullRequests -> RETURN Unhandled
07:   quit_active = (input_mode_for_state == PrsNormal)   // q/Q quits only in PrsNormal
08:   IF quit_active AND key in {'q','Q'} -> RETURN Handled(quit event)
09:   RETURN Handled(prs::handle_prs_mode_key(state, key))

10: FUNCTION handle_prs_mode_key(state, key) -> Option<AppEvent>   // 8-level precedence
11:   // P1: inline composer
12:   IF state.prs_state.inline_state != InlineState::None:   // direct enum sentinel (NOT Option)
13:     RETURN handle_inline_key(state, key)
14:   // P2: agent chooser
15:   IF state.prs_state.agent_chooser is Some:
16:     RETURN handle_agent_chooser_key(state, key)
17:   // P3: search input
18:   IF state.prs_state.search_input_focused:
19:     RETURN handle_search_input_key(state, key)
20:   // P4: filter controls
21:   IF state.prs_state.filter_ui.controls_open:
22:     RETURN handle_filter_controls_key(state, key)   // prs_filter.rs
23:   // P5: PR-global keys
24:   MATCH key:
25:     'p'|'P' -> Some(RefocusPrList)
26:     'a'     -> Some(ExitPrsMode)
27:     Esc     -> RETURN handle_esc_in_prs_mode(state)
28:     '?'|'h'|F1 -> Some(open help with PR bindings)
29:     '/'     -> Some(PrFocusSearchInput)
30:     'f' IF pr_focus == PrList -> Some(PrOpenFilterControls)
31:   // P6: focus-domain handlers (may return None for Tab/Shift+Tab so P7 can claim them)
32:   focus_result = MATCH state.prs_state.pr_focus:
33:     RepoList -> handle_repo_list_key(state, key)
34:     PrList   -> handle_pr_list_key(state, key)
35:     PrDetail -> handle_pr_detail_key(state, key)
36:   IF focus_result is Some -> RETURN focus_result
37:   // P7: pane cycle (only reached when the focus-domain handler returned None for the key;
38:   //     i.e. Tab/Shift+Tab in ANY pane — RepoList, PrList, AND PrDetail all return None for
39:   //     Tab/Shift+Tab so they fall through here — mirror resolve_pane_cycle_key_event. Issue #46:
39a:  //     Tab cycles all three panes including PrDetail; subfocus is on j/k, not Tab).
40:   MATCH key:
41:     Tab       -> RETURN Some(PrCycleFocus)
42:     Shift+Tab -> RETURN Some(PrCycleFocusReverse)
43:   // P8: suppressed reserved keys -> CONSUMED no-op (see "Consumed-no-op semantics" below).
44:   IF key in {'s', Ctrl-d, Ctrl-k, 'l'} -> RETURN None  // wrapped as Handled(None): consumed +
45:   RETURN None                                          //   silently ignored, no event, no effect;
46:                                                        //   never leaks to dashboard; NOT an
47:                                                        //   r/c/e/o read-only case (those return
48:                                                        //   Some(PrShowNotice{kind}) in P6 above).

49: FUNCTION handle_repo_list_key(state, key) -> Option<AppEvent>   // REGRESSION GUARD #47
50:   MATCH key:
51:     Up    -> Some(PrNavigateUp)      // reducer moves repo selection + reloads (pane_focus-free)
52:     Down  -> Some(PrNavigateDown)
53:     Right -> Some(PrCycleFocus)      // arrow pane-cycle forward (mirror issues repo-list L178)
54:     Enter -> None                    // explicit no-op (selection already active)
55:     // Tab/Shift+Tab intentionally fall through to P7 pane cycle (return None here)
56:     _     -> None

57: FUNCTION handle_pr_list_key(state, key) -> Option<AppEvent>
58:   MATCH key:
59:     Up        -> Some(PrNavigateUp)
60:     Down      -> Some(PrNavigateDown)
61:     Left      -> Some(PrCycleFocusReverse)   // arrow pane-cycle back (mirror issues list L124)
62:     Right     -> Some(PrCycleFocus)          // arrow pane-cycle forward (mirror issues list L125)
63:     PageUp    -> Some(PrNavigatePageUp)
64:     PageDown  -> Some(PrNavigatePageDown)
65:     Home      -> Some(PrNavigateHome)
66:     End       -> Some(PrNavigateEnd)
67:     Enter     -> Some(PrListEnter)
68:     'o' IF selected_pr present -> Some(PrOpenInBrowser)   // REQ-PR-012 browser handoff
69:     'o'       -> Some(PrShowNotice{kind: NoSelectionToOpen})  // consume + hint (no silent drop)
70:     // Tab/Shift+Tab intentionally fall through to P7 pane cycle (return None here)
71:     _         -> None

72: FUNCTION handle_pr_detail_key(state, key) -> Option<AppEvent>
73:   subfocus = state.prs_state.detail_subfocus
74:   MATCH key:
75:     Up        -> Some(PrScrollDetailUp)
76:     Down      -> Some(PrScrollDetailDown)
77:     Left      -> Some(PrCycleFocusReverse)   // OPTIONAL parity reverse pane-cycle (NOT the sole
78:                                              //   escape — Tab/Shift+Tab cycle out of detail too);
78a:                                             //   Right is unbound in detail (no rightward pane)
79:     PageUp    -> Some(PrScrollDetailPageUp)
80:     PageDown  -> Some(PrScrollDetailPageDown)
81:     'j'       -> Some(PrDetailSubfocusNext)   // CONSUMED for subfocus traversal (issue #46: Tab is
82:     'k'       -> Some(PrDetailSubfocusPrev)   //   reserved for pane cycle; j/k walk the subfocus).
82a:                                              // Tab/Shift+Tab are NOT matched here -> they return
82b:                                              //   None and fall through to P7 pane cycle.
83:     'c' IF subfocus in {Body, Comment(i), NewComment} -> Some(PrOpenNewCommentComposer)
84:     'c' IF subfocus in {Review(i), Check(i)} -> Some(PrShowNotice{kind: ReadOnlyNoComment})  // consume + hint
85:     'r' IF subfocus == Comment(i) -> Some(PrOpenReplyComposer{comment_index: i})
86:     'r'       -> Some(PrShowNotice{kind: ReadOnlyReplyOnComment})   // consume key + non-blocking hint
87:     'e'       -> Some(PrShowNotice{kind: ReadOnlyNotEditable})      // consume key + non-blocking hint
88:     'o' IF pr_detail present -> Some(PrOpenInBrowser)   // REQ-PR-012: open selected PR in browser
89:     'o'       -> Some(PrShowNotice{kind: NoSelectionToOpen})  // consume + hint (no silent drop)
90:     'S'       -> Some(PrOpenAgentChooser)           // only reachable: no inline control active
91:     _         -> None

92: FUNCTION handle_esc_in_prs_mode(state) -> Option<AppEvent>   // precedence unwind
93:   IF inline_state != InlineState::None  -> Some(PrInlineCancelOrEsc)   // direct enum sentinel
94:   IF agent_chooser present              -> Some(PrAgentChooserCancel)
95:   IF search_input_focused AND search_query nonempty -> Some(PrClearSearch keep-focus)
96:   IF search_input_focused AND search_query empty     -> Some(PrBlurSearchInput)
97:   IF filter_ui.controls_open            -> Some(PrCloseFilterControls)
98:   ELSE                                  -> Some(ExitPrsMode)

99: FUNCTION handle_inline_key(state, key) -> Option<AppEvent>
100:   MATCH key:
101:     Char(c)                  -> Some(PrInlineChar(c))
102:     Enter (plain)            -> Some(PrInlineNewline)
103:     Cmd+Enter / Ctrl+Enter   -> Some(PrInlineSubmit)
104:     Backspace                -> Some(PrInlineBackspace)
105:     Delete                   -> Some(PrInlineDelete)
106:     Left/Right/Up/Down       -> Some(PrInlineCursor*)
107:     Esc                      -> Some(PrInlineCancelOrEsc)
108:     _                        -> None

109: FUNCTION handle_inline_submit(app_state, ctx) -> dispatch (prs_mutation.rs)
110:   // called from dispatch layer when PrInlineSubmit is applied
111:   info = pr_inline_submit_info(app_state)            // scope, pr_number, text, mutation_id
112:   IF info is None -> RETURN (no-op; reducer already cancelled blank)
113:   SET loading.comments-ish pending shown via mutation_pending (already set by reducer)
114:   SPAWN gh task via spawn_gh_task_with_panic:
115:     WORK: result = GhClient.create_pr_comment(owner,name,number,text)
116:           ON Ok(comment)  -> deliver AppEvent::PrCommentCreated{scope,number,mutation_id,comment}
117:           ON Err(e)       -> deliver AppEvent::PrCommentCreateFailed{scope,number,mutation_id,e}
118:     ON_PANIC: deliver PrCommentCreateFailed{... error: "background task panicked"}
119:   RETURN

120: FUNCTION handle_agent_chooser_key(state, key) -> Option<AppEvent>
121:   MATCH key:
122:     Up    -> Some(PrAgentChooserNavigateUp)
123:     Down  -> Some(PrAgentChooserNavigateDown)
124:     Enter -> Some(PrAgentChooserConfirm)
125:     Esc   -> Some(PrAgentChooserCancel)
126:     _     -> None

127: FUNCTION handle_search_input_key(state, key) -> Option<AppEvent>
128:   route_search_key(key):                            // REUSE existing route_search_key
129:     Char(c)   -> Some(PrSetSearchQuery(append c))
130:     Backspace -> Some(PrSetSearchQuery(pop))
131:     Enter     -> Some(PrApplySearch)
132:     Esc       -> RETURN handle_esc_in_prs_mode(state)  // clear-or-blur precedence
133:     _         -> None

134: FUNCTION handle_filter_controls_key(state, key) -> Option<AppEvent>   // REGRESSION GUARD #38/#40
135:   field = state.prs_state.filter_ui.field_index
135a:  // EIGHT fields in cycling order (Finding 1; issue #20 review/workflow signal filters):
135b:  //   0 StateField, 1 DraftField, 2 ReviewField, 3 ChecksField,
135c:  //   4 AuthorField, 5 AssigneeField, 6 ReviewerField, 7 LabelsField.
135d:  //   Tab/Shift+Tab wrap modulo 8. Space cycles the FOUR enumerated fields (state, draft,
135e:  //   review, checks); the FOUR text fields (author, assignee, reviewer, labels) take char input.
136:   MATCH key:
137:     Tab       -> Some(PrFilterNavigateNext)        // field_index = (field + 1) % 8
138:     Shift+Tab -> Some(PrFilterNavigatePrev)        // field_index = (field + 7) % 8
139:     Space IF field == StateField  -> Some(PrCycleFilterState)
140:     Space IF field == DraftField  -> Some(PrCycleDraftFilter)
140a:    Space IF field == ReviewField -> Some(PrCycleReviewFilter)   // issue #20 review signal
140b:    Space IF field == ChecksField -> Some(PrCycleChecksFilter)   // issue #20 workflow signal
141:     Char(c) IF field is text     -> Some(PrUpdateDraftFilter{field, append c})  // live draft
142:     Backspace IF field is text   -> Some(PrUpdateDraftFilter{field, pop})
143:     Enter     -> Some(PrApplyFilter)
144:     Ctrl-c    -> Some(PrClearFilter)
145:     Esc       -> Some(PrCloseFilterControls)
146:     _         -> None

147: FUNCTION dispatch_pr_agent_chooser_confirm(app_state, ctx)   // prs_dispatch.rs
148:   // MIRROR dispatch_agent_chooser_confirm (mod.rs L744-769): read send info BEFORE
149:   //   applying the reducer, THEN apply+persist the confirm event, THEN do side effects.
150:   info = pr_send_info(app_state)                    // PrSendInfo{ agent_id, work_dir, signature, payload }
151:   apply_and_persist(app_state, ctx, PrAgentChooserConfirm)   // reducer closes chooser
152:   IF info is None -> RETURN (no-op; reducer already surfaced any notice)
153:   IF write_pr_prompt(info.work_dir, info.payload) is Err(e) ->
154:        apply PrSendToAgentFailed{error: e}; RETURN  // never silent
155:   launch_pr_agent(info)                             // spawn + attach fresh agent; PERSIST on success
156:   RETURN

157: FUNCTION write_pr_prompt(work_dir, payload) -> Result<(), String>   // mirror write_issue_prompt (mod.rs L820-831)
158:   prompt_dir = work_dir.join(".jefe")
159:   create_dir_all(prompt_dir) MAP Err -> "Failed to create .jefe dir: {e}"
160:   markdown = format_pr_prompt(payload)              // prs_dispatch::format_pr_prompt(&PrSendPayload)
161:   WRITE markdown -> prompt_dir.join("pr-prompt.md") MAP Err -> "Failed to write PR prompt: {e}"
162:   // launch_pr_agent appends the agent instruction flag:
163:   //   "Read and work on the GitHub PR described in .jefe/pr-prompt.md"

164: FUNCTION pr_send_info(app_state) -> Option<PrSendInfo>
165:   // MIRROR issue_send_info (mod.rs L779-808): work_dir + signature come from the AGENT,
166:   //   NOT from the payload.
167:   chooser = prs_state.agent_chooser ELSE None
168:   detail  = prs_state.pr_detail ELSE None
169:   agent   = agent for chooser.selected ELSE None
170:   repo    = repository_by_id(agent.repository_id) ELSE None
171:   focused_comment = focused_pr_comment(app_state) (if detail_subfocus == Comment(i))
172:   work_dir  = agent.work_dir.clone()
173:   signature = launch_signature_for_agent(agent, repo)
174:   payload = GhClient.build_pr_send_payload(repo.slug, detail, focused_comment, repo.issue_base_prompt)
175:   RETURN Some(PrSendInfo{ agent_id, work_dir, signature, payload })

176: FUNCTION format_pr_prompt(payload: &PrSendPayload) -> String   // mirror format_issue_prompt (issues_dispatch.rs L371-413)
177:   // pure rendering of the structured payload into markdown; no I/O
178:   out = ""
179:   APPEND "# GitHub PR #{pr_number}: {pr_title}"
180:   APPEND "**Repository:** {repository}"  + "**State:** {pr_state}"
181:   APPEND "**Branches:** {head_ref} -> {base_ref}"  + "**URL:** {external_url}"
182:   APPEND "## Body" + pr_body
183:   IF review_summary non-empty THEN APPEND "## Reviews" + review_summary lines
184:   IF check_summary  non-empty THEN APPEND "## Checks"  + check_summary lines
185:   IF focused_comment present THEN APPEND "## Focused Comment (by @{focused_comment_author})" + body
186:   IF pr_base_prompt non-empty THEN APPEND "## Instructions" + pr_base_prompt
187:   RETURN out

188: FUNCTION compose_reply_prefill(comment) -> String
189:   RETURN "@" + comment.author_login + " "

190: FUNCTION dispatch_pr_open_in_browser(app_state, ctx)   // prs_dispatch.rs — runtime/side-effect boundary
191:   // Triggered when AppEvent::PrOpenInBrowser is applied (REQ-PR-012 browser handoff).
192:   // ORDERING (mirror dispatch_agent_chooser_confirm, mod.rs L744-769 / the issues
193:   //   send-to-agent precedent): the REDUCER has ALREADY applied the visible notice/state for
194:   //   PrOpenInBrowser (apply_pr_open_in_browser, component-001 L349-357 sets the
195:   //   "opening in browser…" notice) BEFORE this dispatch runs; this function then performs the
196:   //   side effect. The NoSelection key path never reaches PrOpenInBrowser (it emits
197:   //   PrShowNotice{NoSelectionToOpen} at the handler), so here we only resolve repo/number.
198:   info = pr_open_in_browser_info(app_state)   // Result<PrOpenInBrowserInfo, RepoContextError>
199:   MATCH info:
200:     Err(RepoContextError::NoSelection) ->
201:       deliver AppEvent::PrShowNotice{kind: NoSelectionToOpen}; RETURN  // visible notice, no spawn
202:     Err(RepoContextError::InvalidSlug) ->
203:       deliver AppEvent::PrOpenInBrowserFailed{ scope, pr_number,
204:         error: "Configure repository (owner/name) before opening in browser" }; RETURN
205:                                              // categorized visible error — NEVER a silent drop (REQ-PR-013)
206:     Ok(info) -> proceed
207:   SPAWN gh task via spawn_gh_task_with_panic (NEVER blocks the UI thread):
208:     WORK: result = GhClient.open_pull_request_in_browser(info.owner, info.name, info.number)
209:                    // runs `gh pr view <number> --repo <owner>/<name> --web`; gh opens the
210:                    // default browser cross-platform. Reuses the existing gh transport — no
211:                    // bespoke OS opener is introduced (none exists in src today).
212:           ON Ok(())  -> deliver AppEvent::PrOpenedInBrowser{ scope, number }
213:           ON Err(e)  -> deliver AppEvent::PrOpenInBrowserFailed{ scope, number, error: e }
214:     ON_PANIC: deliver PrOpenInBrowserFailed{ scope, number, error: "background task panicked" }
215:   RETURN

216: ENUM RepoContextError { NoSelection, InvalidSlug }   // typed unavailable-context result (REQ-PR-013)

217: FUNCTION pr_open_in_browser_info(app_state) -> Result<PrOpenInBrowserInfo, RepoContextError>
218:   // selected PR is well-defined in BOTH PrList focus (selected list row) and PrDetail focus.
219:   // Repo scope is derived from the SELECTED repository (mirrors issues_dispatch::resolve_gh_repo,
220:   // src/app_input/issues_dispatch.rs L14-36) — there is NO prs_state.scope_id field.
221:   number = selected_pr_number(app_state) ELSE RETURN Err(RepoContextError::NoSelection)
222:   selected_repo = app_state.selected_repository_index
223:                     .and_then(|idx| app_state.repositories.get(idx))
224:                     ELSE RETURN Err(RepoContextError::NoSelection)
225:   (owner, name) = resolve_gh_repo(app_state)         // ("","") when slug missing/malformed
226:   IF owner.is_empty() OR name.is_empty() -> RETURN Err(RepoContextError::InvalidSlug)
227:   scope = current_scope_repo_id(app_state)           // selected_repo.id (mirror L38-46)
228:   RETURN Ok(PrOpenInBrowserInfo{ scope, owner, name, number })

229: // Exclusivity guard (enforced in reducer; restated here for routing intent)
230: FUNCTION exclusivity_guard(state)
231:   ASSERT at most one of {inline_state != None, agent_chooser, filter_controls open, search focused}
232:         is mutable-active at a time; routing precedence (P1..P4) enforces single active control
```

## Routing Precedence Summary

| Priority | Active context | Handler |
|----------|----------------|---------|
| P1 | inline composer (`inline_state != InlineState::None`) | `handle_inline_key` |
| P2 | agent chooser | `handle_agent_chooser_key` |
| P3 | search input focused | `handle_search_input_key` |
| P4 | filter controls open | `handle_filter_controls_key` |
| P5 | PR-global (`p`,`a`,`Esc`,help,`/`,`f`) | `handle_prs_mode_key` body |
| P6 | focus domain | `handle_repo_list_key` / `handle_pr_list_key` / `handle_pr_detail_key` |
| P7 | pane cycle (`Tab`/`Shift+Tab`) — only when P6 returned `None` (ALL panes, incl. PrDetail) | `handle_prs_mode_key` body |
| P8 | suppressed (`s`,`Ctrl-d`,`Ctrl-k`,`l`) | consumed no-op |

## Pane-cycle / subfocus binding table (Tab cycles ALL panes — issue #46)

| Focus | `Tab` | `Shift+Tab` | `j` / `k` | `Left` | `Right` | `Up`/`Down` |
|-------|-------|-------------|-----------|--------|---------|-------------|
| `RepoList` | pane cycle fwd (P7) | pane cycle back (P7) | — | — | `PrCycleFocus` | repo nav (+reload) |
| `PrList` | pane cycle fwd (P7) | pane cycle back (P7) | — | `PrCycleFocusReverse` | `PrCycleFocus` | list nav |
| `PrDetail` | pane cycle fwd (P7) | pane cycle back (P7) | `PrDetailSubfocusNext` / `PrDetailSubfocusPrev` | `PrCycleFocusReverse` (optional parity) | — | detail scroll |

Unlike Issues mode (which consumes `Tab`/`BackTab` for issue-detail subfocus,
`src/app_input/issues.rs` L149-150), PR Mode reserves `Tab`/`Shift+Tab` for inter-pane cycling in
EVERY pane to satisfy issue #46, and moves detail subfocus traversal to `j`/`k`. In `PrDetail` the
focus-domain handler returns `None` for `Tab`/`Shift+Tab`, so they fall through to the SAME P7
pane-cycle fallback used by `RepoList`/`PrList`. Other PR-mode additions are the
read-only/open-in-browser bindings (`o`/`c`/`r`/`e` notices).

## Suppression Guarantees

- `s` (lowercase), `Ctrl-d`, `Ctrl-k`, `l` are consumed in PR Mode (never reach dashboard handlers).
- `a` exits PR Mode (dashboard focus-agents binding suppressed).
- `S` triggers send-to-agent only from PR detail with no inline control active.
- `o` opens the SELECTED pull request in the browser (REQ-PR-012) from PR-list focus or PR-detail
  focus, via `gh pr view <n> --repo <owner>/<name> --web` spawned off-thread (it is the deliberate
  handoff for deferred merge/approve/review-submit operations); when no PR is selected it consumes
  the key and surfaces the `NoSelectionToOpen` notice, and when the repo slug is invalid/missing it
  surfaces a categorized `PrOpenInBrowserFailed` config error (never a silent drop). `o` performs NO
  in-app merge/approve/review-submit mutation.
- `Esc` always resolves through `handle_esc_in_prs_mode` before any split-mode `Esc` behavior.
- `Tab`/`Shift+Tab` cycle panes from EVERY pane including `PrDetail` (issue #46); detail subfocus
  traversal is on `j`/`k` (see binding table above).
- Help (`?`/`h`/`F1`) MUST list `o = open PR in browser` and `Tab = cycle panes` (with the
  `PrDetail` note "j/k = detail subfocus") among PR-Mode bindings.


## No-op + Hint Mechanism (REQ-PR-010 read-only; REQ-PR-013 no-silent-drop)

Invalid `r`/`c`/`e` actions on read-only subfocus (body / review / check / new-comment) MUST NOT
return a bare `None` (a silent drop). Instead the handler returns
`Some(AppEvent::PrShowNotice{ kind })`, which:

1. CONSUMES the key (so it never leaks to dashboard/destructive handlers — same guarantee as the P8
   suppression list), and
2. flows through the normal pipeline (`AppEvent` → `PullRequestsMessage::PrShowNotice` →
   `apply_prs_message` → `apply_pr_show_notice`) so the reducer sets a non-blocking notice that the
   UI renders. This mirrors how Issues Mode surfaces `draft_notice`
   (`src/state/issues_ops.rs` L153) — a state field, set by the reducer, rendered by the screen.

`ReadOnlyHintKind` enum (carried by `PrShowNotice`) and its user-visible text:

| kind | trigger | notice text (non-blocking) |
|------|---------|----------------------------|
| `ReadOnlyReplyOnComment` | `r` on body/review/check/new-comment | "Replies are only available on comments" |
| `ReadOnlyNoComment` | `c` on a review/check item | "Reviews and checks are read-only" |
| `ReadOnlyNotEditable` | `e` anywhere in PR detail | "PR body, reviews, and checks are not editable in v1" |
| `NoSelectionToOpen` | `o` when no PR is selected/loaded | "No pull request selected to open" |

Reducer contract (`apply_pr_show_notice`): set `prs_state.draft_notice = Some(text_for(kind))`;
return `true` (handled). The notice is transient (cleared on the next successful action / mode
change) and never blocks input. Verifier evidence (P10A/P11A) MUST cite that each invalid `r`/`c`/`e`
path returns `Some(PrShowNotice{...})` (NOT `None`) and that a test asserts the resulting
`draft_notice` is populated — proving the hint is surfaced through the event pipeline, not silently
dropped.

The `o`-open-in-browser unavailable-context paths use a TYPED result (`RepoContextError`, c003
L186-198): `NoSelection` surfaces the `NoSelectionToOpen` notice; `InvalidSlug` surfaces a
categorized `PrOpenInBrowserFailed` config error. Neither path can silently return `None`/`Ok` with
no user-visible surface (REQ-PR-013). This mirrors how `issues_dispatch` surfaces an invalid/missing
slug via `missing_detail_repo_event` (`src/app_input/issues_dispatch.rs` L108-109,187-194) rather
than silently dropping the request.


## Consumed-no-op semantics — the EXACT `Option<AppEvent>` contract (REQ-PR-002 suppression)

Every PR key handler returns `Option<AppEvent>`. "Consumption" (stopping the resolver chain so a key
never falls through to dashboard/destructive handlers) is decided at the OUTER `KeyHandling` layer,
NOT by the inner `Option`. `handle_dashboard_prs_key` wraps the entire result of
`handle_prs_mode_key` in `KeyHandling::Handled(...)` (c003 L09), exactly mirroring how
`handle_dashboard_issues_key` consumes every key while in `DashboardIssues`
(`src/app_input/normal.rs` L174,185 — `KeyHandling::Handled(None)`). There is **NO `AppEvent::Noop`
sentinel** in the codebase; suppression is `Handled(None)`, never a sentinel event.

IMPORTANT: a `None` return from a focus-domain handler (P6) for `Tab`/`Shift+Tab` is NOT a silent
drop — it is the deliberate fall-through that lets P7 claim the pane-cycle (RepoList/PrList). The
outer `Handled(...)` wrapper still consumes the key; P7 then produces the pane-cycle event. Only the
P8 suppression list and unbound keys end as `Handled(None)` with no event. The three disjoint
outcomes are:

| Outcome | Handler return | Wrapped as | Effect | Which keys |
|---------|----------------|------------|--------|------------|
| **Consumed + notice** | `Some(AppEvent::PrShowNotice{kind})` | `Handled(Some(PrShowNotice{kind}))` | Key consumed; reducer sets `draft_notice` (user-visible hint) | read-only `r`/`c`/`e` on a read-only subfocus, and `o` with no PR selected (`NoSelectionToOpen`) |
| **Consumed + emit** | `Some(<some other AppEvent>)` | `Handled(Some(event))` | Key consumed; event dispatched | all normal bindings (nav, scroll, enter, arrow/Tab pane cycle, open composer, `o` with a PR present → `PrOpenInBrowser`, …) |
| **Consumed + silently ignored** | `None` | `Handled(None)` | Key consumed (never leaks); NO event, NO user-visible effect | the P8 suppression list (`s`,`Ctrl-d`,`Ctrl-k`,`l`) and the catch-all `_ -> None` arms for keys with no PR-mode binding (Tab/Shift+Tab from RepoList/PrList instead fall through to P7 and DO emit a pane-cycle event) |

Crucial distinctions (the cases the verifier must prove):

- A `None` return from a focus-domain handler for `Tab`/`Shift+Tab` in RepoList/PrList is a
  **fall-through to P7**, which emits `PrCycleFocus`/`PrCycleFocusReverse` — NOT a silent drop.
- A `None` return is the **"consumed + silently ignored"** outcome ONLY for the P8 suppression list
  and genuinely-unbound keys. It is STILL consumed because the outer `Handled(...)` wrapper stops the
  chain; it just carries no event.
- A `None` return is **NEVER** used for the read-only `r`/`c`/`e` cases or the `o`-with-no-selection
  case. Those are the **"consumed + notice"** outcome and MUST return
  `Some(AppEvent::PrShowNotice{kind})` so the user sees a hint (no silent drop — REQ-PR-013).

### Verifier evidence requirement (which test proves which outcome)

- **Pane cycle vs subfocus** (issue #46): `test_tab_cycles_panes_from_every_pane` asserts
  `Tab`/`Shift+Tab` resolve to `PrCycleFocus`/`PrCycleFocusReverse` in ALL THREE panes — `RepoList`,
  `PrList`, AND `PrDetail` (all via P7 fall-through). `test_jk_moves_subfocus_in_pr_detail` asserts
  `j`/`k` resolve to `PrDetailSubfocusNext`/`Prev` in `PrDetail` (and that `Tab`/`Shift+Tab` do NOT
  resolve to subfocus there). `test_left_arrow_optional_reverse_cycle_in_pr_detail` asserts `Left`
  resolves to `PrCycleFocusReverse` in `PrDetail` (optional parity, not the sole escape). This
  DIVERGES from Issues mode (`src/app_input/issues.rs` L149-150 consumes `Tab` for subfocus) to
  satisfy issue #46's explicit "Tab between repo list / PR list / PR detail" requirement.
- **Consumed-with-notice** (must assert `Some(PrShowNotice{kind})` AND `draft_notice` populated):
  `test_c_on_review_or_check_emits_show_notice_not_none`, `test_r_replies_only_on_comment_subfocus`,
  `test_e_on_pr_detail_emits_show_notice_not_none` (P10) and
  `test_o_with_no_selection_emits_show_notice_not_none` (P10), each paired with
  `test_show_notice_sets_draft_notice_for_each_readonly_hint_kind` (P04) which asserts the reducer
  sets `draft_notice` for every `ReadOnlyHintKind` variant. These prove the **notice** path, NOT a
  bare `None`.
- **Invalid-slug open-in-browser** (Finding 7): `test_open_in_browser_invalid_slug_surfaces_error`
  asserts `pr_open_in_browser_info` returns `Err(RepoContextError::InvalidSlug)` for a malformed
  `github_repo` and that the dispatch surfaces a categorized `PrOpenInBrowserFailed` (never `Ok`/no
  surface), and `test_open_in_browser_no_selection_surfaces_notice` asserts
  `Err(RepoContextError::NoSelection)` surfaces `NoSelectionToOpen`.
- **Consumed-and-silently-ignored** (must assert the key is CONSUMED at the `KeyHandling` layer with
  NO emitted event and NO state change): `test_suppressed_keys_ctrl_d_ctrl_k_l_consumed_noop` (P10)
  asserts `s`/`Ctrl-d`/`Ctrl-k`/`l` resolve to `Handled(None)` (consumed, no fallthrough to dashboard
  actions, no `AppEvent`). This proves the **silently-ignored** path is distinct from the notice path
  and never leaks. (Mirrors the existing issues-mode suppression tests
  `test_s_key_suppressed_in_issues_mode` etc. in `src/app_input/issues.rs`.)
