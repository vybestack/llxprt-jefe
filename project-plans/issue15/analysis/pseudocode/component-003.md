# Component 003 Pseudocode — Key Routing + Inline Mutation + Agent Chooser

Plan ID: `PLAN-20260329-ISSUES-MODE`

Requirements: REQ-ISS-001,002,003,004,010,011,012

```text
01: FUNCTION route_issues_mode_key(key_event, state, ctx)
02:   // Priority 1: Inline editor/composer
03:   IF state.issues_state.inline_state != None
04:     RETURN handle_inline_key(key_event, state, ctx)
05:
06:   // Priority 2: Agent chooser
07:   IF state.issues_state.agent_chooser.is_some()
08:     RETURN handle_agent_chooser_key(key_event, state, ctx)
09:
10:   // Priority 3: Search input focused
11:   IF state.issues_state.search_input_focused
12:     RETURN handle_search_input_key(key_event, state, ctx)
13:
14:   // Priority 4: Filter controls open
15:   IF state.issues_state.filter_controls_open
16:     RETURN handle_filter_controls_key(key_event, state, ctx)
17:
18:   // Priority 5: Issues-global unwind and mode controls
19:   MATCH key_event
20:     CASE 'i' -> dispatch RefocusIssueList
21:     CASE 'a' -> dispatch ExitIssuesMode
22:     CASE Esc -> dispatch handle_esc_in_issues_mode
23:     CASE '?' | 'h' | F1 -> dispatch OpenHelp (with issues bindings)
24:
25:   // Priority 6: Focus-domain handlers
26:   MATCH state.issues_state.issue_focus
27:     CASE RepoList -> handle_repo_list_key(key_event, state, ctx)
28:     CASE IssueList -> handle_issue_list_key(key_event, state, ctx)
29:     CASE IssueDetail -> handle_issue_detail_key(key_event, state, ctx)
30:
31:   // Priority 7: Pane-focus cycling outside issue-detail subfocus mode
32:   MATCH key_event
33:     CASE Tab -> dispatch IssuesCycleFocus
34:     CASE Shift+Tab -> dispatch IssuesCycleFocusReverse
35:
36:   // Priority 8: Suppressed keys (consumed as no-op)
37:   MATCH key_event
38:     CASE 's' -> no-op (explicit)
39:     CASE 'S' -> no-op unless IssueDetail focus (handled above)
40:     CASE Ctrl-d | Ctrl-k | 'l' -> no-op (destructive lifecycle suppressed)
41:
42: FUNCTION handle_issue_list_key(key_event, state, ctx)
43:   MATCH key_event
44:     CASE Up -> dispatch IssuesNavigateUp
45:     CASE Down -> dispatch IssuesNavigateDown
46:     CASE PageUp -> dispatch IssuesNavigatePageUp
47:     CASE PageDown -> dispatch IssuesNavigatePageDown
48:     CASE Home -> dispatch IssuesNavigateHome
49:     CASE End -> dispatch IssuesNavigateEnd
50:     CASE Enter -> dispatch IssuesEnter (focus detail)
51:     CASE 'f' -> dispatch OpenFilterControls
52:     CASE '/' -> dispatch FocusSearchInput
53:
54: FUNCTION handle_issue_detail_key(key_event, state, ctx)
55:   MATCH key_event
56:     CASE Up -> dispatch IssuesScrollDetailUp
57:     CASE Down -> dispatch IssuesScrollDetailDown
58:     CASE Tab -> dispatch IssueDetailSubfocusNext
59:     CASE Shift+Tab -> dispatch IssueDetailSubfocusPrev
60:     CASE 'e' ->
61:       IF inline_state == None
62:         IF detail_subfocus == Body -> dispatch OpenInlineEditor(IssueBody)
63:         ELSE IF detail_subfocus == Comment(i) ->
64:           IF comment is editable -> dispatch OpenInlineEditor(Comment(i))
65:           ELSE -> show non-blocking hint "Cannot edit this comment"
66:     CASE 'r' ->
67:       IF inline_state == None
68:         IF detail_subfocus == Comment(i)
69:           -> dispatch OpenReplyComposer(i)
70:         ELSE -> show non-blocking hint "Focus a comment to reply"
71:     CASE 'S' ->
72:       IF inline_state == None
73:         IF agents exist -> dispatch OpenAgentChooser
74:         ELSE -> show "No agents available" message
75:
76: FUNCTION handle_inline_key(key_event, state, ctx)
77:   MATCH key_event
78:     CASE Esc -> dispatch InlineCancelOrEsc
79:     CASE Cmd+Enter | Ctrl+Enter -> dispatch InlineSubmit
80:     CASE Char(c) -> dispatch InlineChar(c)
81:     CASE Backspace -> dispatch InlineBackspace
82:     CASE _ -> consume (no leak to outer handlers)
83:
84: FUNCTION handle_inline_submit(state, ctx)
85:   MATCH state.issues_state.inline_state
86:     CASE Composer { target: NewComment, text }
87:       VALIDATE text non-empty
88:       CALL gh_client.create_comment(owner, repo, issue_number, text)
89:       ON success -> dispatch CommentCreated
90:       ON failure -> dispatch CommentCreateFailed
91:     CASE Composer { target: Reply { comment_index, author }, text }
92:       VALIDATE text non-empty
93:       CALL gh_client.create_comment(owner, repo, issue_number, text)
94:       ON success -> dispatch CommentCreated
95:       ON failure -> dispatch CommentCreateFailed
96:     CASE Editor { target: IssueBody, text }
97:       CALL gh_client.update_issue_body(owner, repo, issue_number, text)
98:       ON success -> dispatch IssueBodyUpdated
99:       ON failure -> dispatch MutationFailed
100:    CASE Editor { target: Comment { comment_index }, text }
101:      GET comment_id from detail.comments[comment_index]
102:      CALL gh_client.update_comment(owner, repo, comment_id, text)
103:      ON success -> dispatch CommentUpdated
104:      ON failure -> dispatch MutationFailed
105:  SET inline_state = None
106:
107: FUNCTION handle_agent_chooser_key(key_event, state, ctx)
108:  MATCH key_event
109:    CASE Up -> dispatch AgentChooserNavigateUp
110:    CASE Down -> dispatch AgentChooserNavigateDown
111:    CASE Enter ->
112:      GET selected_agent from chooser
113:      BUILD send_payload from current issue detail + focused comment + issue_base_prompt
114:      DISPATCH AgentChooserConfirm
115:      DELIVER payload to selected agent runtime
116:      ON success -> dispatch SendToAgentCompleted
117:      ON failure -> dispatch SendToAgentFailed { error }
118:    CASE Esc -> dispatch AgentChooserCancel
119:
120: FUNCTION handle_search_input_key(key_event, state, ctx)
121:  MATCH key_event
122:    CASE Enter -> dispatch ApplySearch
123:    CASE Esc ->
124:      IF search_query non-empty -> dispatch ClearSearch (keep focused)
125:      ELSE -> dispatch BlurSearchInput
126:    CASE Char(c) -> dispatch SetSearchQuery { query: search_query + c }
127:    CASE Backspace -> dispatch SetSearchQuery { query: search_query without last char }
128:
129: FUNCTION handle_filter_controls_key(key_event, state, ctx)
130:  // Navigate filter form fields and apply/clear/cancel
131:  MATCH key_event
132:    CASE Tab -> next filter field
133:    CASE Shift+Tab -> prev filter field
134:    CASE Enter -> dispatch ApplyFilter; close controls
135:    CASE Esc -> dispatch CloseFilterControls (cancel, no commit)
136:    CASE Char(c) -> dispatch UpdateDraftFilter { field: focused_field, value: current_field_value + c }
137:
138: FUNCTION handle_repo_scope_change_in_issues_mode(state, new_repo_id)
139:  DISCARD in-flight requests for prior scope using request_id invalidation
140:  IF inline_state != None
141:    CLEAR inline_state
142:    SET draft_notice = "Unsent draft discarded"
143:  CLEAR issues list, detail, comments, pagination state
144:  SET list_loading = true
145:  EMIT side_effect: load_issue_list(new_repo_id, committed_filter)
146:
147: FUNCTION compose_reply_prefill(comment)
148:  RETURN "@{comment.author_login} "
149:
150: FUNCTION exclusivity_guard(state, requested_control)
151:  IF state.issues_state.inline_state != None
152:    RETURN Err("Another inline control is active")
153:  RETURN Ok
```
