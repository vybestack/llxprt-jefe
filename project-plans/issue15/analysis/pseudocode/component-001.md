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
08:     CASE IssuesPageUp -> handle_issues_page_up(state)
09:     CASE IssuesPageDown -> handle_issues_page_down(state)
10:     CASE IssuesHome -> handle_issues_home(state)
11:     CASE IssuesEnd -> handle_issues_end(state)
12:     CASE IssuesEnter -> handle_issues_enter(state)
13:     CASE IssuesCycleFocus -> cycle_issues_focus(state)
14:     CASE IssuesCycleFocusReverse -> cycle_issues_focus_reverse(state)
15:     CASE IssueListLoaded -> apply_issue_list_loaded(event, state)
16:     CASE IssueListLoadFailed -> apply_issue_list_load_failed(event, state)
17:     CASE IssueListPageLoaded -> apply_issue_list_page_loaded(event, state)
18:     CASE IssueDetailLoaded -> apply_issue_detail_loaded(event, state)
19:     CASE IssueDetailLoadFailed -> apply_issue_detail_load_failed(event, state)
20:     CASE IssueCommentsPageLoaded -> apply_comments_page_loaded(event, state)
21:     CASE IssueCommentsPageFailed -> apply_comments_page_failed(event, state)
22:     CASE OpenFilterControls -> open_filter_controls(state)
23:     CASE CloseFilterControls -> close_filter_controls(state)
24:     CASE ApplyFilter -> apply_committed_filter(state)
25:     CASE ClearFilter -> clear_committed_filter(state)
26:     CASE FocusSearchInput -> focus_search_input(state)
27:     CASE BlurSearchInput -> blur_search_input(state)
28:     CASE ApplySearch -> apply_search_query(state)
29:     CASE ClearSearch -> clear_search_query(state)
30:     CASE Inline* -> dispatch_inline_event(event, state)
31:     CASE AgentChooser* -> dispatch_agent_chooser_event(event, state)
32:   RETURN state + side_effects

33: FUNCTION enter_issues_mode(state)
34:   SAVE prior_agent_focus from current pane_focus + selection indices
35:   SET state.screen_mode = DashboardIssues
36:   SET state.issues_state.active = true
37:   SET state.issues_state.issue_focus = IssueList
38:   CLEAR issues_state data (list, detail, filters, inline controls)
39:   EMIT side_effect: load_issue_list(selected_repository_id, default_filter)
40:   RETURN state

41: FUNCTION exit_issues_mode(state)
42:   SET state.screen_mode = Dashboard
43:   SET state.issues_state.active = false
44:   DISCARD unsent inline drafts with notice if present
45:   RESTORE prior_agent_focus if valid:
46:     IF prior_focus.target exists AND target is focusable
47:       RESTORE pane_focus + selection indices
48:     ELSE
49:       SET pane_focus = Agents, selection = default
50:   CLEAR issues_state transient data
51:   RETURN state

52: FUNCTION handle_issues_navigate_up(state)
53:   MATCH state.issues_state.issue_focus
54:     CASE RepoList -> move repo selection up (existing behavior)
55:                      EMIT side_effect: reload_issues_for_new_scope
56:     CASE IssueList -> move issue selection up in current page
57:                       IF new_index < 0 THEN clamp to 0
58:                       EMIT side_effect: load_detail_for_selected
59:     CASE IssueDetail -> scroll detail content up
60:   RETURN state

61: FUNCTION handle_issues_navigate_down(state)
62:   MATCH state.issues_state.issue_focus
63:     CASE RepoList -> move repo selection down (existing behavior)
64:                      EMIT side_effect: reload_issues_for_new_scope
65:     CASE IssueList -> move issue selection down
66:                       IF at_last_loaded AND has_more_issues
67:                         EMIT side_effect: load_next_page
68:                       EMIT side_effect: load_detail_for_selected
69:     CASE IssueDetail -> scroll detail content down
70:   RETURN state

71: FUNCTION cycle_issues_focus(state)
72:   MATCH state.issues_state.issue_focus
73:     CASE RepoList -> SET IssueList
74:     CASE IssueList -> SET IssueDetail
75:     CASE IssueDetail -> SET RepoList
76:   RETURN state

77: FUNCTION cycle_issues_focus_reverse(state)
78:   MATCH state.issues_state.issue_focus
79:     CASE RepoList -> SET IssueDetail
80:     CASE IssueList -> SET RepoList
81:     CASE IssueDetail -> SET IssueList
82:   RETURN state

83: FUNCTION apply_issue_list_loaded(event, state)
84:   VALIDATE event.scope matches current selected_repository_id
85:   IF scope mismatch THEN discard (stale response)
86:   SET state.issues_state.issues = event.issues
87:   SET state.issues_state.list_page_cursor = event.cursor
88:   SET state.issues_state.has_more_issues = event.has_more
89:   SET state.issues_state.list_loading = false
90:   IF issues non-empty
91:     SET selected_issue_index = 0
92:     EMIT side_effect: load_detail_for_selected
93:   ELSE
94:     SET selected_issue_index = None
95:     CLEAR issue_detail
96:   RETURN state

97: FUNCTION apply_issue_list_page_loaded(event, state)
98:   VALIDATE event.scope matches current
99:   APPEND event.issues to state.issues_state.issues
100:  UPDATE list_page_cursor and has_more_issues
101:  SET list_loading = false
102:  RETURN state

103: FUNCTION apply_issue_detail_loaded(event, state)
104:  VALIDATE event.scope and event.issue_number match current selection
105:  SET state.issues_state.issue_detail = Some(event.detail)
106:  SET state.issues_state.detail_loading = false
107:  SET state.issues_state.detail_subfocus = Body
108:  RETURN state

109: FUNCTION apply_comments_page_loaded(event, state)
110:  VALIDATE scope match
111:  APPEND event.comments to existing detail.comments (stable order)
112:  UPDATE comments_cursor and has_more_comments
113:  SET comments_loading = false
114:  RETURN state

115: FUNCTION handle_esc_in_issues_mode(state)
116:  IF state.issues_state.inline_state != None
117:    CANCEL inline control -> InlineState::None
118:  ELSE IF state.issues_state.agent_chooser.is_some()
119:    CLOSE agent chooser -> None
120:  ELSE IF state.issues_state.search_input_focused AND search_query non-empty
121:    CLEAR search_query, keep search_input_focused
122:  ELSE IF state.issues_state.search_input_focused AND search_query empty
123:    SET search_input_focused = false
124:  ELSE IF state.issues_state.filter_controls_open
125:    CLOSE filter_controls (cancel, no commit)
126:  ELSE
127:    exit_issues_mode(state)
128:  RETURN state

129: FUNCTION handle_issues_enter(state)
130:  IF issue_focus == IssueList AND selected_issue exists
131:    SET issue_focus = IssueDetail
132:  RETURN state

133: FUNCTION handle_detail_subfocus_tab(state)
134:  IF detail has comments
135:    MATCH detail_subfocus
136:      CASE Body -> SET Comment(0)
137:      CASE Comment(i) ->
138:        IF i + 1 < comments.len() THEN SET Comment(i + 1)
139:        ELSE SET NewComment
140:      CASE NewComment -> SET Body
141:  ELSE (no comments)
142:    MATCH detail_subfocus
143:      CASE Body -> SET NewComment
144:      CASE NewComment -> SET Body
145:  RETURN state

146: FUNCTION handle_detail_subfocus_shift_tab(state)
147:  IF detail has comments
148:    MATCH detail_subfocus
149:      CASE Body -> SET NewComment
150:      CASE Comment(0) -> SET Body
151:      CASE Comment(i) -> SET Comment(i - 1)
152:      CASE NewComment -> SET Comment(comments.len() - 1)
153:  ELSE
154:    MATCH detail_subfocus
155:      CASE Body -> SET NewComment
156:      CASE NewComment -> SET Body
157:  RETURN state

158: FUNCTION selection_after_filter_change(state, new_issues)
159:  IF current selected issue number exists in new_issues
160:    SET selected_issue_index to matching index
161:  ELSE IF new_issues non-empty
162:    SET selected_issue_index = 0
163:  ELSE
164:    SET selected_issue_index = None
165:  RETURN state
```
