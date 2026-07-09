# Component 004 Pseudocode — Message-Bus Wiring + Dispatch Routing (PR Mode)

Plan ID: `PLAN-20260624-PR-MODE`

Requirements: REQ-PR-001,003,006,007,009,010,011,012,013, REQ-PR-NFR-001,002,003

This component covers the typed message-bus surface (`src/messages.rs`,
`src/messages/prs_conversion.rs`) and the dispatch-layer routing of `AppMessage::PullRequests`
(`src/app_input/mod.rs`, `prs_dispatch.rs`, `prs_list_dispatch.rs`). It is additive: the legacy
`AppEvent` facade remains the reducer's source via `AppEvent::from(message)`. Issue #15 predates
this explicit message-bus pseudocode; PR Mode documents it because the bus now exists.

```text
01: ENUM MessageDomain { ..existing.., PullRequests }      // add variant

02: ENUM PullRequestsMessage {                              // mirror IssuesMessage shape
03:   // lifecycle
04:   EnterMode, ExitMode, RefocusList,
05:   // navigation/focus
06:   Navigate(NavDir), Enter, CycleFocus, CycleFocusReverse,
07:   ScrollDetail(ScrollDir), DetailSubfocusNext, DetailSubfocusPrev,
08:   // data loading (Box large payloads)
09:   ListLoaded{ scope_repo_id, filter: Box<PrFilter>, request_id, pull_requests, cursor, has_more },
10:   ListLoadFailed{ scope_repo_id, request_id, error },
11:   ListPageLoaded{ scope_repo_id, request_id, pull_requests, cursor, has_more },
12:   DetailLoaded{ scope_repo_id, pr_number, request_id, detail: Box<PullRequestDetail> },
13:   DetailLoadFailed{ scope_repo_id, pr_number, request_id, error },
14:   CommentsPageLoaded{ scope_repo_id, pr_number, request_id, comments, cursor, has_more },
15:   CommentsPageFailed{ scope_repo_id, pr_number, request_id, error },
16:   // filter/search
17:   OpenFilterControls, CloseFilterControls, ApplyFilter, ClearFilter,
18:   FilterNavigate(Dir), CycleFilterState, CycleDraftFilter, CycleReviewFilter, CycleChecksFilter,
18a:  UpdateDraftFilter{ field, value },   // CycleReview/CycleChecks: issue #20 review/workflow signals
19:   FocusSearchInput, BlurSearchInput, SetSearchQuery{ query }, ApplySearch, ClearSearch,
20:   // inline mutation
21:   OpenNewCommentComposer, OpenReplyComposer{ comment_index },
22:   Inline(InlineMsg),
23:   CommentCreated{ scope_repo_id, pr_number, mutation_id, comment },
24:   CommentCreateFailed{ scope_repo_id, pr_number, mutation_id, error },
25:   MutationFailed{ scope_repo_id, pr_number, mutation_id, error },
26:   // read-only no-op hint (REQ-PR-010/013): consumed key + non-blocking notice, no silent None
27:   ShowNotice(ReadOnlyHintKind),
28:   // send-to-agent
29:   OpenAgentChooser, AgentChooserNavigate(Dir), AgentChooserConfirm, AgentChooserCancel,
30:   SendToAgentCompleted, SendToAgentFailed{ error },
31:   // open-in-browser (REQ-PR-012): deferred-op handoff to the browser via gh CLI
32:   OpenInBrowser,
33:   OpenedInBrowser{ scope_repo_id, pr_number },
34:   OpenInBrowserFailed{ scope_repo_id, pr_number, error },
35: }

36: ENUM AppMessage { ..existing.., PullRequests(PullRequestsMessage) }   // add variant

37: FUNCTION AppMessage::domain(self)
38:   MATCH self: ..existing.., PullRequests(_) -> MessageDomain::PullRequests

39: FUNCTION AppMessage::route(self) -> MessageRoute
40:   RETURN MessageRoute{ domain: self.domain(), name: self.name() }

41: FUNCTION AppMessage::name(self) -> &'static str
42:   MATCH self: ..existing.., PullRequests(m) -> m.name()

43: MACRO message_names! invocation for PullRequestsMessage
44:   // generates PullRequestsMessage::name() mapping each variant to a stable &'static str

45: // ---- conversions (src/messages/prs_conversion.rs) ----

46: IMPL From<AppEvent> for AppMessage   (extend existing)
47:   // route PR-domain AppEvents into AppMessage::PullRequests
48:   IF is_prs_event(event) -> RETURN AppMessage::PullRequests(from_prs_event(event))

49: FUNCTION from_prs_event(event) -> PullRequestsMessage
50:   RETURN PullRequestsMessage::from_app_event(event)

51: FUNCTION PullRequestsMessage::from_app_event(event) -> PullRequestsMessage
52:   MATCH event:
53:     EnterPrsMode -> EnterMode
54:     ExitPrsMode -> ExitMode
55:     RefocusPrList -> RefocusList
56:     PrNavigateUp -> Navigate(Up) ... (Down/PageUp/PageDown/Home/End)
57:     PrListEnter -> Enter
58:     PrCycleFocus -> CycleFocus ; PrCycleFocusReverse -> CycleFocusReverse
59:     PrScrollDetail* -> ScrollDetail(dir)
60:     PrDetailSubfocusNext/Prev -> DetailSubfocusNext/Prev
61:     PrListLoaded{..} -> ListLoaded{..}            // 1:1 payload move
61a:    PrCycleReviewFilter -> CycleReviewFilter ; PrCycleChecksFilter -> CycleChecksFilter  // issue #20 signals
62:     PrShowNotice(kind) -> ShowNotice(kind)        // read-only hint (REQ-PR-010/013)
63:     PrOpenInBrowser -> OpenInBrowser              // open-in-browser (REQ-PR-012)
64:     PrOpenedInBrowser{..} -> OpenedInBrowser{..}
65:     PrOpenInBrowserFailed{..} -> OpenInBrowserFailed{..}
66:     ... (all data/filter/search/inline/mutation/agent variants mapped 1:1)
67:     _ -> UNREACHABLE for non-PR events (guarded by is_prs_event)

68: IMPL From<PullRequestsMessage> for AppEvent
69:   MATCH message:
70:     EnterMode -> EnterPrsMode ; ExitMode -> ExitPrsMode ; RefocusList -> RefocusPrList
71:     Navigate(dir) -> PrNavigate{dir} ; Enter -> PrListEnter
72:     CycleFocus -> PrCycleFocus ; CycleFocusReverse -> PrCycleFocusReverse
73:     ScrollDetail(dir) -> PrScrollDetail{dir}
74:     DetailSubfocusNext/Prev -> PrDetailSubfocusNext/Prev
75:     ListLoaded{..} -> PrListLoaded{..} ; ListPageLoaded{..} -> PrListPageLoaded{..}
76:     DetailLoaded{..} -> PrDetailLoaded{..} ; CommentsPageLoaded{..} -> PrCommentsPageLoaded{..}
77:     *Failed{..} -> Pr*Failed{..}
78:     OpenFilterControls -> PrOpenFilterControls ... (all filter/search variants)
78a:    CycleReviewFilter -> PrCycleReviewFilter ; CycleChecksFilter -> PrCycleChecksFilter  // issue #20 signals
79:     OpenNewCommentComposer -> PrOpenNewCommentComposer ; OpenReplyComposer{i} -> Pr...{i}
80:     Inline(m) -> PrInline{m} ; CommentCreated{..} -> PrCommentCreated{..}
81:     ShowNotice(kind) -> PrShowNotice(kind)        // read-only hint (REQ-PR-010/013)
82:     OpenInBrowser -> PrOpenInBrowser              // open-in-browser (REQ-PR-012)
83:     OpenedInBrowser{..} -> PrOpenedInBrowser{..} ; OpenInBrowserFailed{..} -> PrOpenInBrowserFailed{..}
84:     OpenAgentChooser -> PrOpenAgentChooser ... (all agent variants)
85:   // bidirectional: AppEvent::from(message) is the reducer source of truth

86: // ---- reducer-hub arm (src/state/mod.rs apply_message) ----

87: FUNCTION apply_message(state, message)              // extend existing match
88:   MATCH message.domain():
89:     ..existing arms..
90:     PullRequests:
91:       handled = state.apply_prs_message(message)     // returns bool
92:       debug_assert!(handled, "unhandled PullRequests message {route}")
93:   finalize_message(route)                            // rebuild ids, normalize selection
94:   RETURN state

95: // terminal_blocks(): PR nav messages are NOT terminal-blocked (mirror Issues nav)

96: // ---- dispatch-layer routing (src/app_input/mod.rs dispatch_app_message) ----

97: FUNCTION dispatch_app_message(app_state, ctx, message)   // extend BIG match
98:   MATCH message:
99:     PullRequests(Navigate*|Enter|CycleFocus|...) where load needed:
100:      -> dispatch_prs_navigation(app_state, ctx, message)
101:    PullRequests(EnterMode|RefocusList|ApplyFilter|ClearFilter|ApplySearch):
102:      -> apply_and_persist(message); prs_list_dispatch::dispatch_pr_list_reload(app_state, ctx)
103:    PullRequests(Enter):
104:      -> apply_and_persist(message); prs_dispatch::load_pr_detail_for_selection(app_state, ctx)
105:    PullRequests(ScrollDetailDown|ScrollDetailPageDown):
106:      -> update_pr_detail_viewport_rows(app_state, ctx)   // set viewport prop from layout
107:         apply_and_persist(message)
108:         prs_dispatch::load_more_pr_comments(app_state, ctx)   // lazy paginate near bottom
109:    PullRequests(AgentChooserConfirm):
110:      -> apply_and_persist(message); dispatch_pr_agent_chooser_confirm(app_state, ctx)
111:    PullRequests(Inline(Submit)):
112:      -> apply_and_persist(message); prs_mutation::handle_pr_inline_submit(app_state, ctx)
113:    PullRequests(OpenInBrowser):                  // REQ-PR-012 — side-effect, off-thread
114:      -> apply_and_persist(message)               // reducer may set a transient "opening…" notice
115:         prs_dispatch::dispatch_pr_open_in_browser(app_state, ctx)   // spawns gh pr view --web
116:    PullRequests(_) fallthrough:                  // incl. OpenedInBrowser / OpenInBrowserFailed
117:      -> apply_and_persist(AppEvent::from(message))   // pure reducer path (sets/clears notice)
118:  RETURN

119: FUNCTION dispatch_prs_navigation(app_state, ctx, message)   // mirror dispatch_issues_navigation
120:   apply_and_persist(message)                         // reducer moves selection/repo scope
121:   refresh_pr_navigation(app_state, ctx)              // detail preview + repo-scope refresh
122:   RETURN

123: FUNCTION refresh_pr_navigation(app_state, ctx)
124:   refresh_repo_scope_if_changed(app_state, ctx)      // reset list + reload on repo change
125:   refresh_pr_preview_if_changed(app_state, ctx)      // load detail for newly selected PR
126:   RETURN

127: FUNCTION request_pr_list_reload(app_state, ctx)      // prs_list_dispatch.rs
128:   slug = selected repo.github_repo
129:   IF validate_repo_slug(slug) is Err -> set scoped config message; RETURN (no spawn)
130:   IF NOT gh authenticated -> set auth message; RETURN
131:   SET loading.list = true; record list_reload_pending{scope, request_id}
132:   SPAWN via spawn_gh_task_with_panic:
133:     WORK: r = GhClient.list_pull_requests(owner,name,committed_filter,None)
134:           Ok -> deliver PrListLoaded{scope, filter, request_id, prs, cursor, has_more}
135:           Err -> deliver PrListLoadFailed{scope, request_id, error}
136:     ON_PANIC: clear loading.list; deliver PrListLoadFailed{... "panic"}
137:   RETURN

138: FUNCTION load_pr_detail_for_selection(app_state, ctx)    // prs_dispatch.rs
139:   pr = selected PR ELSE RETURN
140:   SET loading.detail = true; record detail_pending{scope, pr_number, request_id}
141:   SPAWN via spawn_gh_task_with_panic:
142:     WORK: r = GhClient.get_pull_request_detail(owner,name,pr.number)
143:           Ok(detail) -> deliver PrDetailLoaded{scope, pr_number, request_id, detail}
144:           Err -> deliver PrDetailLoadFailed{scope, pr_number, request_id, error}
145:     ON_PANIC: clear loading.detail; deliver PrDetailLoadFailed{... "panic"}
146:   RETURN

147: FUNCTION load_more_pr_comments(app_state, ctx)
148:   IF NOT pr_detail.has_more_comments OR comments_page_pending present -> RETURN
149:   SET loading.comments = true; record comments_page_pending{scope, pr_number, request_id, cursor}
150:   SPAWN via spawn_gh_task_with_panic:
151:     WORK: r = GhClient.list_pr_comments(owner,name,pr.number,cursor,PR_COMMENT_PAGE_SIZE)
151a:          // PR comments fetch via repository.pullRequest(number:).comments, NOT the issue
151b:          // list_comments (repository.issue(number:) is NULL for a PR number — P00A §2d).
152:           Ok -> deliver PrCommentsPageLoaded{scope, pr_number, request_id, comments, cursor, more}
153:           Err -> deliver PrCommentsPageFailed{scope, pr_number, request_id, error}
154:     ON_PANIC: clear loading.comments; deliver PrCommentsPageFailed{... "panic"}
155:   RETURN

156: FUNCTION update_pr_detail_viewport_rows(app_state, ctx)   // REGRESSION GUARD #37/#39
157:   rows = jefe::layout::prs_detail_viewport_rows(available_height)   // prop from layout module
158:   SET app_state.prs_state.detail_viewport_rows = rows    // NOT read independently in scroll math
159:   RETURN

160: FUNCTION dispatch_pr_open_in_browser(app_state, ctx)     // prs_dispatch.rs (REQ-PR-012)
160a:  // ORDERING: the reducer apply_pr_open_in_browser (component-001 L349-357) ALREADY set the
160b:  //   "opening in browser…" notice in the dispatch arm (c004 L113-115) BEFORE this runs.
161:   info = pr_open_in_browser_info(app_state)               // see component-003 lines 217-228
162:                                                           // -> Result<PrOpenInBrowserInfo, RepoContextError>
163:   MATCH info:
164:     Err(RepoContextError::NoSelection) ->
165:       deliver PrShowNotice{ kind: NoSelectionToOpen }; RETURN   // visible notice, no spawn
166:     Err(RepoContextError::InvalidSlug) ->
167:       deliver PrOpenInBrowserFailed{ scope, pr_number,
168:         error: "Configure repository (owner/name) before opening in browser" }; RETURN  // never silent
169:     Ok(info) -> proceed
170:   SPAWN via spawn_gh_task_with_panic (off-thread; never blocks input):
171:     WORK: r = GhClient.open_pull_request_in_browser(info.owner, info.name, info.number)  // gh pr view --web
172:           Ok(()) -> deliver PrOpenedInBrowser{ scope, pr_number }
173:           Err(e) -> deliver PrOpenInBrowserFailed{ scope, pr_number, error: e }
174:     ON_PANIC: deliver PrOpenInBrowserFailed{ scope, pr_number, error: "background task panicked" }
175:   RETURN
```

## Full Dispatch Chain (PR Mode)

```text
key event
  -> handle_normal_key_event (normal.rs L85)
     -> handle_dashboard_prs_key (new)  [screen == DashboardPullRequests]
        -> prs::handle_prs_mode_key  -> Option<AppEvent>
  -> (or) resolve_mode_key 'p'/'P'  -> Some(EnterPrsMode)   [screen == Dashboard]
        |
        v
  AppEvent  --(AppEvent.into())-->  AppMessage::PullRequests(PullRequestsMessage)
        |
        v
  dispatch_app_message (app_input/mod.rs L420)
     |-- side-effecting arms: prs_list_dispatch / prs_dispatch / prs_mutation
     |     -> spawn_gh_task_with_panic (off-thread gh I/O) -> deliver data AppEvent
     |-- pure arms: apply_and_persist(AppEvent::from(message))
        |
        v
  AppState::apply_message (state/mod.rs L342)
     -> AppMessage::PullRequests arm -> apply_prs_message (bool)
        -> apply_prs_event chained-OR over prs_ops / prs_load_ops / prs_inline_ops / prs_mutation_ops
     -> finalize_message(route)
        |
        v
  render: build_screen_element -> PullRequestsScreen (DashboardPullRequests)
```

## Conversion Round-Trip Invariant

For every PR `AppEvent` variant `E`:
`AppEvent::from(PullRequestsMessage::from_app_event(E)) == E` (structurally equal payloads).
This round-trip is asserted by behavioral tests in the DOMAIN-STATE slice — RED in P04, GREEN in
P05 — because the conversion (`src/messages/prs_conversion.rs`) is stubbed in P03 and the P05 reducer
hub `apply_prs_message` already depends on `AppEvent::from(message)`. It is NOT tested or implemented
in the message-bus key-routing phases (P10/P11), which only handle key routing/dispatch (finding #1).
