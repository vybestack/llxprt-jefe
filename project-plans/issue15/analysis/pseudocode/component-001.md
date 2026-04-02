# Component 001 Pseudocode — Issues Mode State + Event Reducer

Plan ID: `PLAN-20260329-ISSUES-MODE`

Requirements: REQ-ISS-001,002,003,004,005,006,007,008,010,014

```text
01: FUNCTION dispatch_issues_event(event, state)
02:   MATCH event
03:     CASE EnterIssuesMode -> enter_issues_mode(state)
04:     CASE ExitIssuesMode -> exit_issues_mode(state)
05:     CASE RefocusIssueList -> set state.issues_state.issue_focus = IssueList
06:     CASE IssuesNavigateUp -> handle_issues_navigate_up(state)
07:     CASE IssuesNavigateDown -> handle_issues_navigate_down(state)
08:     CASE IssuesNavigatePageUp -> handle_issues_page_up(state)
09:     CASE IssuesNavigatePageDown -> handle_issues_page_down(state)
10:     CASE IssuesNavigateHome -> handle_issues_home(state)
11:     CASE IssuesNavigateEnd -> handle_issues_end(state)
12:     CASE IssuesEnter -> handle_issues_enter(state)
13:     CASE IssuesCycleFocus -> cycle_issues_focus(state)
14:     CASE IssuesCycleFocusReverse -> cycle_issues_focus_reverse(state)
15:     CASE IssueDetailSubfocusNext -> handle_detail_subfocus_tab(state)
16:     CASE IssueDetailSubfocusPrev -> handle_detail_subfocus_shift_tab(state)
17:     CASE IssueListLoaded -> apply_issue_list_loaded(event, state)
18:     CASE IssueListLoadFailed -> apply_issue_list_load_failed(event, state)
19:     CASE IssueListPageLoaded -> apply_issue_list_page_loaded(event, state)
20:     CASE IssueDetailLoaded -> apply_issue_detail_loaded(event, state)
21:     CASE IssueDetailLoadFailed -> apply_issue_detail_load_failed(event, state)
22:     CASE IssueCommentsPageLoaded -> apply_comments_page_loaded(event, state)
23:     CASE IssueCommentsPageFailed -> apply_comments_page_failed(event, state)
24:     CASE OpenFilterControls -> open_filter_controls(state)
25:     CASE CloseFilterControls -> close_filter_controls(state)
26:     CASE ApplyFilter -> apply_committed_filter(state)
27:     CASE ClearFilter -> clear_committed_filter(state)
28:     CASE FocusSearchInput -> focus_search_input(state)
29:     CASE BlurSearchInput -> blur_search_input(state)
30:     CASE SetSearchQuery -> set_search_query(event, state)
31:     CASE ApplySearch -> apply_search_query(state)
32:     CASE ClearSearch -> clear_search_query(state)
33:     CASE UpdateDraftFilter -> update_draft_filter(event, state)
34:     CASE OpenNewCommentComposer -> open_new_comment_composer(state)
35:     CASE OpenReplyComposer -> open_reply_composer(event, state)
36:     CASE OpenInlineEditor -> open_inline_editor(event, state)
37:     CASE InlineChar -> apply_inline_char(event, state)
38:     CASE InlineBackspace -> apply_inline_backspace(state)
39:     CASE InlineSubmit -> apply_inline_submit(state)
40:     CASE InlineCancelOrEsc -> apply_inline_cancel(state)
41:     CASE CommentCreated -> apply_comment_created(event, state)
42:     CASE CommentCreateFailed -> apply_comment_create_failed(event, state)
43:     CASE IssueBodyUpdated -> apply_issue_body_updated(event, state)
44:     CASE CommentUpdated -> apply_comment_updated(event, state)
45:     CASE MutationFailed -> apply_mutation_failed(event, state)
46:     CASE OpenAgentChooser -> open_agent_chooser(state)
47:     CASE AgentChooserNavigateUp -> agent_chooser_navigate_up(state)
48:     CASE AgentChooserNavigateDown -> agent_chooser_navigate_down(state)
49:     CASE AgentChooserConfirm -> apply_agent_chooser_confirm(state)
50:     CASE AgentChooserCancel -> apply_agent_chooser_cancel(state)
51:     CASE SendToAgentCompleted -> apply_send_to_agent_completed(state)
52:     CASE SendToAgentFailed -> apply_send_to_agent_failed(event, state)
53:   RETURN state + side_effects

54: FUNCTION enter_issues_mode(state)
55:   SAVE prior_agent_focus from current pane_focus + selection indices
56:   SET state.screen_mode = DashboardIssues
57:   SET state.issues_state.active = true
58:   SET state.issues_state.issue_focus = IssueList
59:   CLEAR issues_state data (list, detail, filters, inline controls)
60:   EMIT side_effect: load_issue_list(selected_repository_id, default_filter)
61:   RETURN state

62: FUNCTION exit_issues_mode(state)
63:   SET state.screen_mode = Dashboard
64:   SET state.issues_state.active = false
65:   DISCARD unsent inline drafts with notice if present
66:   RESTORE prior_agent_focus if valid:
67:     IF prior_focus.target exists AND target is focusable
68:       RESTORE pane_focus + selection indices
69:     ELSE
70:       SET pane_focus = Agents, selection = default
71:   CLEAR issues_state transient data
72:   RETURN state

73: FUNCTION handle_issues_navigate_up(state)
74:   MATCH state.issues_state.issue_focus
75:     CASE RepoList -> move repo selection up (existing behavior)
76:                      EMIT side_effect: reload_issues_for_new_scope
77:     CASE IssueList -> move issue selection up in current page
78:                       IF new_index < 0 THEN clamp to 0
79:                       EMIT side_effect: load_detail_for_selected
80:     CASE IssueDetail -> scroll detail content up
81:   RETURN state

82: FUNCTION handle_issues_navigate_down(state)
83:   MATCH state.issues_state.issue_focus
84:     CASE RepoList -> move repo selection down (existing behavior)
85:                      EMIT side_effect: reload_issues_for_new_scope
86:     CASE IssueList -> move issue selection down
87:                       IF at_last_loaded AND has_more_issues
88:                         EMIT side_effect: load_next_page
89:                       EMIT side_effect: load_detail_for_selected
90:     CASE IssueDetail -> scroll detail content down
91:   RETURN state

92: FUNCTION cycle_issues_focus(state)
93:   MATCH state.issues_state.issue_focus
94:     CASE RepoList -> SET IssueList
95:     CASE IssueList -> SET IssueDetail
96:     CASE IssueDetail -> SET RepoList
97:   RETURN state

98: FUNCTION cycle_issues_focus_reverse(state)
99:   MATCH state.issues_state.issue_focus
100:     CASE RepoList -> SET IssueDetail
101:     CASE IssueList -> SET RepoList
102:     CASE IssueDetail -> SET IssueList
103:   RETURN state

104: FUNCTION apply_issue_list_loaded(event, state)
105:   VALIDATE event.scope_repo_id matches current selected_repository_id
106:   VALIDATE event.request_id matches current outstanding list request
107:   IF validation fails THEN discard (stale response)
108:   SET state.issues_state.issues = event.issues
109:   SET state.issues_state.list_cursor = event.cursor
110:   SET state.issues_state.has_more_issues = event.has_more
111:   SET state.issues_state.list_loading = false
112:   IF issues non-empty
113:     SET selected_issue_index = 0
114:     EMIT side_effect: load_detail_for_selected
115:   ELSE
116:     SET selected_issue_index = None
117:     CLEAR issue_detail
118:   RETURN state

119: FUNCTION apply_issue_list_page_loaded(event, state)
120:   VALIDATE event.scope_repo_id matches current selected_repository_id
121:  VALIDATE event.request_id matches current outstanding list request
122:  IF validation fails THEN discard (stale response)
123:  APPEND event.issues to state.issues_state.issues
124:  UPDATE list_cursor and has_more_issues
125:  SET list_loading = false
126:  RETURN state

127: FUNCTION apply_issue_detail_loaded(event, state)
128:  VALIDATE event.scope_repo_id matches current selected_repository_id
129:  VALIDATE event.request_id matches current outstanding detail request
130:  VALIDATE event.issue_number matches current selection
131:  IF validation fails THEN discard (stale response)
132:  SET state.issues_state.issue_detail = Some(event.detail)
133:  SET state.issues_state.detail_loading = false
134:  SET state.issues_state.detail_subfocus = Body
135:  RETURN state

136: FUNCTION apply_comments_page_loaded(event, state)
137:  VALIDATE event.scope_repo_id matches current selected_repository_id
138:  VALIDATE event.request_id matches current outstanding comments request
139:  VALIDATE event.issue_number matches current detail issue number
140:  IF validation fails THEN discard (stale response)
141:  ASSERT event.comments are older than currently loaded visible comments
142:  APPEND older comments after currently loaded comments in stable timeline order
143:  DO NOT reorder or replace already loaded comments
144:  UPDATE comments_cursor and has_more_comments
145:  SET comments_loading = false
146:  RETURN state

147: FUNCTION handle_esc_in_issues_mode(state)
148:  IF state.issues_state.inline_state != None
149:    CANCEL inline control -> InlineState::None
150:  ELSE IF state.issues_state.agent_chooser.is_some()
151:    CLOSE agent chooser -> None
152:  ELSE IF state.issues_state.search_input_focused AND search_query non-empty
153:    CLEAR search_query, keep search_input_focused
154:  ELSE IF state.issues_state.search_input_focused AND search_query empty
155:    SET search_input_focused = false
156:  ELSE IF state.issues_state.filter_controls_open
157:    CLOSE filter_controls (cancel, no commit)
158:  ELSE
159:    exit_issues_mode(state)
160:  RETURN state

161: FUNCTION handle_issues_enter(state)
162:  IF issue_focus == IssueList AND selected_issue exists
163:    SET issue_focus = IssueDetail
164:  RETURN state

165: FUNCTION handle_detail_subfocus_tab(state)
166:  IF detail has comments
167:    MATCH detail_subfocus
168:      CASE Body -> SET Comment(0)
169:      CASE Comment(i) ->
170:        IF i + 1 < comments.len() THEN SET Comment(i + 1)
171:        ELSE SET NewComment
172:      CASE NewComment -> SET Body
173:  ELSE (no comments)
174:    MATCH detail_subfocus
175:      CASE Body -> SET NewComment
176:      CASE NewComment -> SET Body
177:  RETURN state

178: FUNCTION handle_detail_subfocus_shift_tab(state)
179:  IF detail has comments
180:    MATCH detail_subfocus
181:      CASE Body -> SET NewComment
182:      CASE Comment(0) -> SET Body
183:      CASE Comment(i) -> SET Comment(i - 1)
184:      CASE NewComment -> SET Comment(comments.len() - 1)
185:  ELSE
186:    MATCH detail_subfocus
187:      CASE Body -> SET NewComment
188:      CASE NewComment -> SET Body
189:  RETURN state

190: FUNCTION selection_after_filter_change(state, new_issues)
191:  IF current selected issue number exists in new_issues
192:    SET selected_issue_index to matching index
193:  ELSE IF new_issues non-empty
194:    SET selected_issue_index = 0
195:  ELSE
196:    SET selected_issue_index = None
197:  RETURN state
```
