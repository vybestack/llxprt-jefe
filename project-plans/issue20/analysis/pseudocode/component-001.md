# Component 001 Pseudocode — Pull Requests Mode State + Event Reducer

Plan ID: `PLAN-20260624-PR-MODE`

Requirements: REQ-PR-001,002,003,004,005,006,007,008,009,010,011,012,013,014

This component is the pure state reducer. It owns all PR-mode state transitions and emits
side-effect requests (loads/mutations) that the dispatch layer fulfills off-thread. It performs
no I/O.

Note on `inline_state` (Finding 4): `PullRequestsState.inline_state` stores `InlineState`
DIRECTLY (not `Option<InlineState>`), mirroring `IssuesState.inline_state` in the current source
(`src/state/types.rs`: `InlineState` derives `Default` with a `#[default] None` variant). "No
active composer" is the `InlineState::None` sentinel; presence is tested via
`inline_state != InlineState::None` (exactly like `src/state/issues_ops.rs` L152).

```text
01: FUNCTION dispatch_prs_event(event, state)
02:   MATCH event
03:     CASE EnterPrsMode -> enter_prs_mode(state)
04:     CASE ExitPrsMode -> exit_prs_mode(state)
05:     CASE RefocusPrList -> set state.prs_state.pr_focus = PrList
06:     CASE PrNavigateUp -> handle_pr_navigate_up(state)
07:     CASE PrNavigateDown -> handle_pr_navigate_down(state)
08:     CASE PrNavigatePageUp -> handle_pr_page_up(state)
09:     CASE PrNavigatePageDown -> handle_pr_page_down(state)
10:     CASE PrNavigateHome -> handle_pr_home(state)
11:     CASE PrNavigateEnd -> handle_pr_end(state)
12:     CASE PrListEnter -> handle_pr_enter(state)
13:     CASE PrCycleFocus -> cycle_pr_focus(state)
14:     CASE PrCycleFocusReverse -> cycle_pr_focus_reverse(state)
15:     CASE PrScrollDetailUp -> scroll_detail(state, -1)
16:     CASE PrScrollDetailDown -> scroll_detail(state, +1)
17:     CASE PrScrollDetailPageUp -> scroll_detail_page(state, -1)
18:     CASE PrScrollDetailPageDown -> scroll_detail_page(state, +1)
19:     CASE PrDetailSubfocusNext -> handle_detail_subfocus_next(state)
20:     CASE PrDetailSubfocusPrev -> handle_detail_subfocus_prev(state)
21:     CASE PrListLoaded -> apply_pr_list_loaded(event, state)
22:     CASE PrListLoadFailed -> apply_pr_list_load_failed(event, state)
23:     CASE PrListPageLoaded -> apply_pr_list_page_loaded(event, state)
24:     CASE PrDetailLoaded -> apply_pr_detail_loaded(event, state)
25:     CASE PrDetailLoadFailed -> apply_pr_detail_load_failed(event, state)
26:     CASE PrCommentsPageLoaded -> apply_pr_comments_page_loaded(event, state)
27:     CASE PrCommentsPageFailed -> apply_pr_comments_page_failed(event, state)
28:     CASE PrOpenFilterControls -> open_filter_controls(state)
29:     CASE PrCloseFilterControls -> close_filter_controls(state)
30:     CASE PrApplyFilter -> apply_committed_filter(state)
31:     CASE PrClearFilter -> clear_committed_filter(state)
32:     CASE PrFilterNavigateNext -> filter_field_next(state)
33:     CASE PrFilterNavigatePrev -> filter_field_prev(state)
34:     CASE PrCycleFilterState -> cycle_filter_state(state)
35:     CASE PrCycleDraftFilter -> cycle_draft_filter(state)
35a:    CASE PrCycleReviewFilter -> cycle_review_filter(state)   // issue #20 review signal
35b:    CASE PrCycleChecksFilter -> cycle_checks_filter(state)   // issue #20 workflow signal
36:     CASE PrUpdateDraftFilter -> update_draft_filter(event, state)
37:     CASE PrFocusSearchInput -> focus_search_input(state)
38:     CASE PrBlurSearchInput -> blur_search_input(state)
39:     CASE PrSetSearchQuery -> set_search_query(event, state)
40:     CASE PrApplySearch -> apply_search_query(state)
41:     CASE PrClearSearch -> clear_search_query(state)
42:     CASE PrOpenNewCommentComposer -> open_new_comment_composer(state)
43:     CASE PrOpenReplyComposer -> open_reply_composer(event, state)
44:     CASE PrInlineChar -> apply_inline_char(event, state)
45:     CASE PrInlineNewline -> apply_inline_newline(state)
46:     CASE PrInlineBackspace -> apply_inline_backspace(state)
47:     CASE PrInlineDelete -> apply_inline_delete(state)
48:     CASE PrInlineCursorLeft/Right/Up/Down -> apply_inline_cursor(event, state)
49:     CASE PrInlineSubmit -> apply_inline_submit(state)
50:     CASE PrInlineCancelOrEsc -> apply_inline_cancel(state)
51:     CASE PrCommentCreated -> apply_comment_created(event, state)
52:     CASE PrCommentCreateFailed -> apply_comment_create_failed(event, state)
53:     CASE PrMutationFailed -> apply_mutation_failed(event, state)
54:     CASE PrOpenAgentChooser -> open_agent_chooser(state)
55:     CASE PrAgentChooserNavigateUp -> agent_chooser_navigate_up(state)
56:     CASE PrAgentChooserNavigateDown -> agent_chooser_navigate_down(state)
57:     CASE PrAgentChooserConfirm -> apply_agent_chooser_confirm(state)
58:     CASE PrAgentChooserCancel -> apply_agent_chooser_cancel(state)
59:     CASE PrSendToAgentCompleted -> apply_send_to_agent_completed(state)
60:     CASE PrSendToAgentFailed -> apply_send_to_agent_failed(event, state)
61:     CASE PrShowNotice -> apply_pr_show_notice(state, event.kind)   // read-only hint (REQ-010/013)
62:     CASE PrOpenInBrowser -> apply_pr_open_in_browser(state)        // REQ-PR-012 (side-effect spawned by dispatch)
63:     CASE PrOpenedInBrowser -> apply_pr_opened_in_browser(event, state)
64:     CASE PrOpenInBrowserFailed -> apply_pr_open_in_browser_failed(event, state)
65:   RETURN state + side_effects

66: FUNCTION enter_prs_mode(state)
67:   SAVE state.prs_state.prior_agent_focus FROM current pane_focus + selection indices
68:   SET state.screen_mode = DashboardPullRequests
69:   SET state.prs_state.active = true
70:   SET state.prs_state.pr_focus = PrList
71:   CLEAR prs_state data (pull_requests, pr_detail, draft+committed filter, inline, agent_chooser,
72:         search, errors, cursors, pending guards) to defaults
73:   SET state.prs_state.inline_state = InlineState::None       // Finding 4: direct enum, not Option
74:   SET state.prs_state.committed_filter.state = Some(Open)    // default scope = open PRs
75:   EMIT side_effect: request_pr_list_reload(scope_repo_id, committed_filter, new request_id)
76:   RETURN state

77: FUNCTION exit_prs_mode(state)
78:   SET state.screen_mode = Dashboard
79:   SET state.prs_state.active = false
80:   IF inline_state != InlineState::None THEN discard with draft_notice; SET inline_state = None
81:   RESTORE prior_agent_focus IF valid:
82:     IF prior_focus token exists AND its target still exists AND target is focusable
83:       RESTORE pane_focus + selection indices
84:     ELSE
85:       SET pane_focus = Agents, selection = default
86:   CLEAR prs_state transient data
87:   RETURN state

88: FUNCTION reset_prs_for_repo_change(state)        // called from select_repository_by_index
89:   // staleness: bump all request-id counters so in-flight responses are discarded
90:   INVALIDATE list_reload_pending, list_page_pending, detail_pending, comments_page_pending
91:   CLEAR pull_requests, pr_detail, list_cursor, has_more_prs, error
92:   SET selected_pr_index = None
93:   SET detail_scroll_offset = 0, detail_subfocus = Body
94:   IF inline_state != InlineState::None THEN                      // Finding 4: direct enum sentinel
95:     SET draft_notice = "Draft discarded (repo changed)"; SET inline_state = InlineState::None
96:   PRESERVE committed_filter + search_query (scope-independent user intent)
97:   EMIT side_effect: request_pr_list_reload(new scope_repo_id, committed_filter, new request_id)
98:   RETURN state

99: FUNCTION handle_pr_navigate_up(state)
100:   IF pr_focus == RepoList -> RETURN navigate_repo_up_in_prs_mode(state)   // see line 137
101:   IF pr_focus == PrList:
102:     IF selected_pr_index > 0 THEN selected_pr_index -= 1
103:     SET list_scroll_offset = list_first_visible_index(selected_pr_index,    // selection-follow
104:                                pull_requests.len(), list_viewport_rows)     // see lines 182-189
105:     EMIT side_effect: request_pr_detail(scope, selected pr.number, new request_id)
106:   IF pr_focus == PrDetail -> scroll_detail(state, -1)
107:   RETURN state

108: FUNCTION handle_pr_navigate_down(state)
109:   IF pr_focus == RepoList -> RETURN navigate_repo_down_in_prs_mode(state)
110:   IF pr_focus == PrList:
111:     IF selected_pr_index + 1 < pull_requests.len() THEN selected_pr_index += 1
112:     SET list_scroll_offset = list_first_visible_index(selected_pr_index,    // selection-follow
113:                                pull_requests.len(), list_viewport_rows)     // see lines 182-189
114:     IF selected_pr_index == pull_requests.len() - 1 AND has_more_prs AND no list_page_pending
115:       EMIT side_effect: request_pr_list_page(scope, list_cursor, new request_id)  // lazy-load
116:     EMIT side_effect: request_pr_detail(scope, selected pr.number, new request_id)
117:   IF pr_focus == PrDetail -> scroll_detail(state, +1)
118:   RETURN state

119: // handle_pr_page_up / handle_pr_page_down / handle_pr_home / handle_pr_end follow the same
120: // shape when pr_focus == PrList: clamp selected_pr_index by +/- one viewport page (or to 0 /
121: // last), THEN recompute list_scroll_offset = list_first_visible_index(selected_pr_index,
122: // pull_requests.len(), list_viewport_rows) (lines 182-189) so the selected row stays on-screen
123: // (#55), and request detail for the new selection. PageDown additionally triggers lazy
124: // page-load (lines 114-115) when it lands on the last loaded row with has_more_prs.

125: // ---- SHARED REPO-NAV HELPER (Finding 5; REGRESSION GUARD #47, no pane_focus dependency) ----
126: // The repo Up/Down navigation is IDENTICAL between Issues and PR modes except for which
127: // reset_*_for_repo_change runs. The current source duplicates this logic in
128: // src/state/issues_ops.rs::navigate_repo_up_in_issues_mode (L122-131) and
129: // navigate_repo_down_in_issues_mode (L137-148). This plan EXTRACTS the shared selection-move
130: // (remember -> move within visible indices -> restore) into ONE helper on AppState that BOTH
131: // modes call, so the logic is not copied. The helper returns whether the selection changed; the
132: // caller then runs its own mode-specific reset. The Issues-mode functions are refactored to call
133: // the same helper (deliverable in P09/P11; verifier asserts PR mode does NOT define a private copy).
134: FUNCTION move_repo_selection(state, direction) -> bool    // shared; lives in src/state/mod.rs
135:   indices = visible_repository_indices(state)              // existing helper (mod.rs L194)
136:   IF indices empty THEN RETURN false
137:   cur = selected_repository_visible_index(state) ELSE 0    // existing helper (mod.rs L207)
138:   target = MATCH direction:
139:              Up   -> (cur > 0) ? cur - 1 : cur
140:              Down -> (cur + 1 < indices.len()) ? cur + 1 : cur
141:   IF target == cur THEN RETURN false                       // at a boundary: no change
142:   remember_selected_agent_for_current_repo(state)          // existing helper (mod.rs L130)
143:   SET state.selected_repository_index = Some(indices[target])  // selection drives reload, NOT pane_focus
144:   restore_selected_agent_for_current_repo(state)           // existing helper (mod.rs L152)
145:   RETURN true

146: FUNCTION navigate_repo_up_in_prs_mode(state)              // thin wrapper over the shared helper
147:   IF move_repo_selection(state, Up) THEN reset_prs_for_repo_change(state)  // reload happens here
148:   RETURN state

149: FUNCTION navigate_repo_down_in_prs_mode(state)            // thin wrapper over the shared helper
150:   IF move_repo_selection(state, Down) THEN reset_prs_for_repo_change(state)
151:   RETURN state
152: // (Issues mode is refactored symmetrically: navigate_repo_up/down_in_issues_mode become
153: //  `if move_repo_selection(self, Dir) { self.reset_issues_for_repo_change() }`.)

154: FUNCTION cycle_pr_focus(state)                    // Tab from RepoList/PrList (P7 fall-through)
155:   pr_focus = MATCH pr_focus: RepoList->PrList, PrList->PrDetail, PrDetail->RepoList
156:   RETURN state
157:   // cycle_pr_focus_reverse is the inverse ordering. NOTE (issue #46): Tab/Shift+Tab REACH these
158:   // fns from EVERY pane including PrDetail — the key layer does NOT consume Tab/Shift+Tab for
159:   // subfocus (detail subfocus is on j/k, component-003 L81-82), so PrDetail->RepoList here is
160:   // reached via Tab just like RepoList->PrList and PrList->PrDetail; Shift+Tab reverses. Left is
161:   // an OPTIONAL parity reverse pane-cycle out of PrDetail (-> PrCycleFocusReverse). This DIVERGES
162:   // from Issues mode (src/app_input/issues.rs L149-150 consumes Tab for subfocus) to satisfy #46.

163: FUNCTION handle_pr_enter(state)
164:   IF pr_focus == PrList AND selected_pr_index is Some:
165:     SET pr_focus = PrDetail
166:     SET detail_subfocus = Body
167:   IF pr_focus == RepoList -> no-op (selection already active)
168:   RETURN state

169: FUNCTION scroll_detail(state, delta)
170:   max_offset = max_scroll_offset(state)            // derived from ACTUAL rendered length
171:   detail_scroll_offset = clamp(detail_scroll_offset + delta, 0, max_offset)
172:   RETURN state

173: FUNCTION max_scroll_offset(state)                  // REGRESSION GUARD #37/#39
174:   rendered_lines = total rendered content lines of unified detail view (real, not heuristic)
175:   viewport = state.prs_state.detail_viewport_rows  // prop from layout module, not crossterm
176:   RETURN saturating_sub(rendered_lines, viewport)

177: // ---- NEW SHARED LIST-VIEWPORT / SELECTION-FOLLOW HELPER (REQ-PR-006, #54/#55) ----
178: // Pure function. No equivalent exists in the codebase today: issue_list.rs renders ALL rows
179: // with no offset, and ScrollableText windows TEXT lines (not list rows). This helper is a
180: // NEW shared deliverable: pure fns in src/layout.rs ONLY (the shared leaf module importable by
180a:// BOTH the state reducers and ui — see plan/00-overview.md lines 114-119). There is NO
180b:// src/ui/components/list_viewport.rs file (finding #2/#7); the helpers do NOT live in a UI module.
181: // Consumed by pr_list.rs. It computes the first visible row so the selected row is on-screen.
182: FUNCTION list_first_visible_index(selected_index, len, viewport_rows) -> usize
183:   IF len == 0 OR viewport_rows == 0 THEN RETURN 0
184:   // clamp inputs defensively (selected may briefly exceed len during async updates)
185:   sel = min(selected_index, len - 1)
186:   IF sel < viewport_rows THEN RETURN 0                      // top of list: no scroll
187:   // keep selected row as the LAST visible row when scrolling down past the viewport bottom
188:   max_first = saturating_sub(len, viewport_rows)            // never scroll past the last full page
189:   RETURN min(sel - viewport_rows + 1, max_first)

190: FUNCTION list_visible_window(rows, selected_index, viewport_rows) -> slice
191:   first = list_first_visible_index(selected_index, rows.len(), viewport_rows)
192:   last  = min(first + viewport_rows, rows.len())            // N loaded rows render exactly N when they fit
193:   RETURN rows[first .. last]                                // #54: never silently drop a row
194: // Invariant (tested in P13): for any selected_index in [0,len), the returned window ALWAYS
195: // contains selected_index, and window length == min(viewport_rows, len). pr_list renders only
196: // this window using viewport_rows = prs_pane_rows(...) from the typed layout module.
197: // VIEWPORT SOURCING (purity): the reducer never reads crossterm. state.prs_state.list_viewport_rows
198: // is refreshed at the DISPATCH boundary by an update_prs_list_viewport_rows(...) helper that reads
199: // crossterm::size() once and writes prs_pane_rows(...) into state — mirroring the existing
200: // update_detail_viewport_rows pattern (src/app_input/mod.rs). Reducers then read the stored value.

201: FUNCTION handle_detail_subfocus_next(state)   // bound to 'j' in PrDetail (NOT Tab — issue #46)
202:   // order: Body -> Review(0..n) -> Check(0..n) -> Comment(0..n) -> NewComment -> Body
203:   // skip empty sections; clamp indices to loaded vectors
204:   next = next_nonempty_subfocus(current detail_subfocus, reviews, checks, comments)
205:   SET detail_subfocus = next
206:   ENSURE active subfocus visible (auto-scroll viewport to reveal it)
207:   RETURN state
208:   // handle_detail_subfocus_prev (bound to 'k') traverses the same graph in reverse

209: FUNCTION apply_pr_list_loaded(event, state)
210:   VALIDATE event.scope_repo_id == current scope_repo_id ELSE RETURN state (discard stale)
211:   VALIDATE event.request_id == list_reload_pending.request_id ELSE RETURN state (discard stale)
212:   CLEAR list_reload_pending; SET loading.list = false; CLEAR error
213:   SET pull_requests = sort_default(event.pull_requests)   // updated desc, number asc tiebreak
214:   SET list_cursor = event.cursor; has_more_prs = event.has_more
215:   IF pull_requests nonempty:
216:     SET selected_pr_index = Some(0); SET list_scroll_offset = 0   // first row visible (#55)
217:     EMIT side_effect: request_pr_detail(scope, pull_requests[0].number, new request_id)
218:   ELSE:
219:     SET selected_pr_index = None; SET list_scroll_offset = 0; CLEAR pr_detail (scoped empty)
220:   RETURN state
221: // NOTE: on filter/search re-selection (keep-by-number or fall-to-first) the reducer likewise
222: // recomputes list_scroll_offset via list_first_visible_index (lines 182-189) so the retained
223: // or newly-selected row stays on-screen (#55).

224: FUNCTION apply_pr_list_page_loaded(event, state)
225:   VALIDATE scope + request_id ELSE discard
226:   CLEAR list_page_pending
227:   APPEND event.pull_requests to pull_requests (preserve existing rows + selection)
228:   SET list_cursor = event.cursor; has_more_prs = event.has_more
229:   RETURN state

230: FUNCTION apply_pr_detail_loaded(event, state)
231:   VALIDATE scope + pr_number matches selected + request_id ELSE discard stale
232:   CLEAR detail_pending; SET loading.detail = false
233:   SET pr_detail = event.detail (includes reviews, checks, comments, cursors, external_url)
234:   SET detail_scroll_offset = 0; detail_subfocus = Body
235:   RETURN state

236: FUNCTION apply_pr_comments_page_loaded(event, state)
237:   VALIDATE scope + pr_number + request_id ELSE discard
238:   CLEAR comments_page_pending; SET loading.comments = false
239:   APPEND event.comments to pr_detail.comments in STABLE timeline order (never reorder/replace)
240:   SET pr_detail.comments_cursor = event.cursor; has_more_comments = event.has_more
241:   RETURN state

242: FUNCTION apply_pr_list_load_failed(event, state)
243:   VALIDATE scope + request_id ELSE discard
244:   CLEAR list_reload_pending; SET loading.list = false
245:   SET error = user_message(event.error)            // never silent; categorized message
246:   RETURN state
247:   // apply_pr_detail_load_failed / apply_pr_comments_page_failed are analogous,
248:   // setting scoped error + clearing the matching pending guard + preserving loaded data

249: FUNCTION open_filter_controls(state)
250:   PRECONDITION pr_focus == PrList ELSE RETURN state (no-op)
251:   SET filter_ui.controls_open = true; field_index = 0
252:   COPY committed_filter -> draft_filter
253:   RETURN state

254: FUNCTION update_draft_filter(event, state)         // REGRESSION GUARD #38/#40
255:   MATCH event.field:
256:     Query/Author/Assignee/Reviewer/Labels -> SET corresponding draft_filter text
257:     // text entry updates draft immediately so the user sees changes
258:   RETURN state

259: FUNCTION cycle_filter_state(state)   // Space on state field
260:   draft_filter.state = next of [Open, Closed, Merged, All] (wrap)
261:   RETURN state

262: FUNCTION cycle_draft_filter(state)   // Space on draft field
263:   draft_filter.is_draft = next of [None(any), Some(true)(drafts), Some(false)(ready)] (wrap)
264:   RETURN state

264a: FUNCTION cycle_review_filter(state)   // Space on review-decision field (issue #20 review signal)
264b:   draft_filter.review_decision = next of
264c:     [Any, Approved, ChangesRequested, ReviewRequired, None] (wrap)
264d:   RETURN state

264e: FUNCTION cycle_checks_filter(state)   // Space on checks-status field (issue #20 workflow signal)
264f:   draft_filter.checks_status = next of [Any, Success, Failing, Pending] (wrap)
264g:   RETURN state

265: FUNCTION apply_committed_filter(state)             // Enter / Apply
266:   COPY draft_filter -> committed_filter
267:   SET filter_ui.controls_open = false
268:   CALL reload_pr_list_for_filter_change(state)
269:   RETURN state

270: FUNCTION clear_committed_filter(state)             // Ctrl-c / Clear
271:   RESET committed_filter to default (state = Some(Open), review_decision = Any,
271a:        checks_status = Any, all text/labels empty, is_draft = None)
272:   RESET draft_filter likewise
273:   CALL reload_pr_list_for_filter_change(state)
274:   RETURN state

275: FUNCTION reload_pr_list_for_filter_change(state)
276:   remember_selected_number = pull_requests[selected_pr_index].number if any
277:   CLEAR list_cursor, has_more_prs
278:   EMIT side_effect: request_pr_list_reload(scope, committed_filter, new request_id)
279:   // selection_after_filter_change applies on PrListLoaded:
280:   //   keep by remembered number if present; else 0; else None
281:   RETURN state

282: FUNCTION apply_search_query(state)                 // Enter in search input
283:   committed_filter.query_text = trim(search_query)
284:   SET search_input_focused = false
285:   CALL reload_pr_list_for_filter_change(state)
286:   RETURN state

287: FUNCTION clear_search_query(state)
288:   SET search_query = ""
289:   committed_filter.query_text = ""
290:   CALL reload_pr_list_for_filter_change(state)
291:   RETURN state

292: FUNCTION open_new_comment_composer(state)          // REGRESSION GUARD #56
293:   PRECONDITION pr_focus == PrDetail ELSE no-op
294:   GUARD exclusivity: IF inline_state != InlineState::None THEN RETURN state (no-op)  // Finding 4
295:   SET inline_state = InlineState::Composer{ target: ComposerTarget::NewComment, text: "", cursor: 0 }
296:   SET detail_subfocus = NewComment              // open composer moves subfocus to NewComment
297:   AUTO-SCROLL detail viewport so the composer is revealed (offset = max_scroll_offset)
298:   RETURN state

299: FUNCTION open_reply_composer(event, state)
300:   PRECONDITION detail_subfocus == Comment(index) ELSE RETURN apply_pr_show_notice(
301:     state, ReadOnlyHintKind::ReadOnlyReplyOnComment)   // surfaced notice, not a silent no-op
302:   GUARD exclusivity: IF inline_state != InlineState::None THEN RETURN state (no-op)  // Finding 4
303:   prefill = "@" + comments[index].author_login + " "
304:   SET inline_state = InlineState::Composer{ target: ComposerTarget::Reply{comment_index: index,
305:        author: comments[index].author_login}, text: prefill, cursor: len(prefill) }
306:   AUTO-SCROLL to reveal the reply field
307:   RETURN state

308: FUNCTION apply_inline_submit(state)
309:   text = inline_state buffer (from the Composer variant)   // Finding 4: match InlineState::Composer
310:   IF text is blank THEN SET inline_state = InlineState::None (cancel composer); RETURN state
311:   mutation_id = next_mutation_id++
312:   SET mutation_pending = PrMutationPending{scope, mutation_id, target}
313:   EMIT side_effect: request_create_pr_comment(scope, pr_number, text, mutation_id)
314:   // composer remains until CommentCreated/Failed; UI shows pending
315:   RETURN state

316: FUNCTION apply_comment_created(event, state)        // REGRESSION GUARD #56 (follow viewport)
317:   VALIDATE scope + pr_number + mutation_id ELSE discard
318:   CLEAR mutation_pending; SET inline_state = InlineState::None     // Finding 4: direct enum
319:   APPEND event.comment to pr_detail.comments (stable order)
320:   SET detail_subfocus = Comment(index of new comment)
321:   AUTO-SCROLL viewport so the new comment is visible
322:   RETURN state

323: FUNCTION apply_comment_create_failed(event, state)
324:   VALIDATE scope + mutation_id ELSE discard
325:   CLEAR mutation_pending; PRESERVE inline_state (keep user's draft text in the Composer variant)
326:   SET error = user_message(event.error)
327:   RETURN state

328: FUNCTION apply_inline_cancel(state)                 // Esc / cancel composer
329:   SET inline_state = InlineState::None                            // Finding 4: direct enum sentinel
330:   RETURN state

331: FUNCTION open_agent_chooser(state)
332:   PRECONDITION pr_focus == PrDetail AND inline_state == InlineState::None   // Finding 4
333:   IF no agents available THEN SET draft_notice "No agents available"; RETURN state
334:   SET agent_chooser = AgentChooserState::new(default selection)
335:   RETURN state

336: FUNCTION apply_agent_chooser_confirm(state)
337:   // dispatch layer reads selection + builds payload + writes .jefe/pr-prompt.md + launches
338:   SET agent_chooser = None
339:   EMIT side_effect: send_pr_to_agent(scope, selected pr, selected agent, focused comment?)
340:   RETURN state

341: FUNCTION apply_send_to_agent_failed(event, state)
342:   SET error = user_message(event.error)            // never silent
343:   RETURN state

344: FUNCTION apply_pr_show_notice(state, kind)         // REQ-PR-010 read-only; REQ-PR-013 no-silent-drop
345:   // Invoked for invalid r/c/e on read-only subfocus; surfaces a non-blocking hint through
346:   // the event pipeline instead of returning a bare None. Mirrors issues draft_notice.
347:   SET prs_state.draft_notice = Some(text_for(kind))   // text per component-003 hint table
348:   RETURN state                                        // (handled = true at the event-op layer)

349: FUNCTION apply_pr_open_in_browser(state)           // REQ-PR-012 — pure reducer half
350:   // The actual browser launch is a side effect spawned by the dispatch layer
351:   // (dispatch_pr_open_in_browser, component-003 L190-228 / component-004 L160-175). This reducer
352:   // half runs FIRST (applied+persisted in the dispatch arm BEFORE the async spawn, c004 L113-115);
352a:  // it only sets a transient non-blocking notice — performs NO I/O and mutates NO selection.
353:   IF selected_pr_number(state) is None:
354:     SET prs_state.draft_notice = Some("No pull request selected to open")  // NoSelectionToOpen
355:   ELSE:
356:     SET prs_state.draft_notice = Some("Opening pull request in browser…")
357:   RETURN state

358: FUNCTION apply_pr_opened_in_browser(event, state)  // success ack (never silent)
359:   VALIDATE event.scope_repo_id == current scope ELSE RETURN state (discard stale)
360:   SET prs_state.draft_notice = Some("Opened PR #" + event.pr_number + " in browser")
361:   RETURN state

362: FUNCTION apply_pr_open_in_browser_failed(event, state)   // REQ-PR-013 no silent error
363:   VALIDATE event.scope_repo_id == current scope ELSE RETURN state (discard stale)
364:   SET prs_state.error = user_message(event.error)    // categorized, user-visible
365:   RETURN state
```

## Reducer-Hub Wiring (state/mod.rs)

```text
366: FUNCTION apply_prs_message(state, message) -> bool
367:   // special-case ApplySearch to commit trimmed query before generic conversion (mirror issues)
368:   IF message == PrApplySearch:
369:     CALL apply_search_query(state); RETURN true
370:   ELSE:
371:     event = AppEvent::from(message)
372:     RETURN apply_prs_event(state, event)

373: FUNCTION apply_prs_event(state, event) -> bool    // chained-OR over op modules
374:   RETURN apply_pr_scroll_event(state,event)
375:        OR apply_pr_lifecycle_event(state,event)
376:        OR apply_pr_filter_event(state,event)
377:        OR apply_pr_inline_open_event(state,event)
378:        OR apply_pr_inline_event(state,event)
379:        OR apply_pr_mutation_event(state,event)
380:        OR apply_pr_load_event(state,event)
381:        OR apply_pr_agent_chooser_event(state,event)
382:        OR apply_pr_notice_event(state,event)       // PrShowNotice -> apply_pr_show_notice (#010/#013)
383:        OR apply_pr_open_browser_event(state,event) // PrOpenInBrowser/Opened/Failed (REQ-PR-012)
384:        OR apply_pr_error_event(state,event)
385:   // returns false if no op handled it -> debug_assert in apply_message catches gaps
```

## Selection-After-Filter-Change

```text
386: FUNCTION selection_after_filter_change(prev_selected_number, new_list)
387:   IF prev_selected_number present in new_list -> select that index
388:   ELSE IF new_list nonempty -> select index 0
389:   ELSE -> select None (scoped empty state)
```
