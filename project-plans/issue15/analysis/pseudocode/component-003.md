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
18:   // Priority 5: Focus-domain handlers
19:   MATCH state.issues_state.issue_focus
20:     CASE RepoList -> handle_repo_list_key(key_event, state, ctx)
21:     CASE IssueList -> handle_issue_list_key(key_event, state, ctx)
22:     CASE IssueDetail -> handle_issue_detail_key(key_event, state, ctx)
23:
24:   // Priority 6: Issues-global handlers
25:   MATCH key_event
26:     CASE 'i' -> dispatch RefocusIssueList
27:     CASE 'a' -> dispatch ExitIssuesMode
28:     CASE Esc -> dispatch handle_esc_in_issues_mode
29:     CASE Tab -> dispatch IssuesCycleFocus
30:     CASE Shift+Tab -> dispatch IssuesCycleFocusReverse
31:     CASE '?' | 'h' | F1 -> dispatch OpenHelp (with issues bindings)
32:
33:   // Priority 7: Suppressed keys (consumed as no-op)
34:   MATCH key_event
35:     CASE 's' -> no-op (explicit)
36:     CASE 'S' -> no-op unless IssueDetail focus (handled above)
37:     CASE Ctrl-d | Ctrl-k | 'l' -> no-op (destructive lifecycle suppressed)
38:
39: FUNCTION handle_issue_list_key(key_event, state, ctx)
40:   MATCH key_event
41:     CASE Up -> dispatch IssuesNavigateUp
42:     CASE Down -> dispatch IssuesNavigateDown
43:     CASE PageUp -> dispatch IssuesPageUp
44:     CASE PageDown -> dispatch IssuesPageDown
45:     CASE Home -> dispatch IssuesHome
46:     CASE End -> dispatch IssuesEnd
47:     CASE Enter -> dispatch IssuesEnter (focus detail)
48:     CASE 'f' -> dispatch OpenFilterControls
49:     CASE '/' -> dispatch FocusSearchInput

50: FUNCTION handle_issue_detail_key(key_event, state, ctx)
51:   MATCH key_event
52:     CASE Up -> dispatch IssuesScrollDetailUp
53:     CASE Down -> dispatch IssuesScrollDetailDown
54:     CASE Tab -> dispatch handle_detail_subfocus_tab
55:     CASE Shift+Tab -> dispatch handle_detail_subfocus_shift_tab
56:     CASE 'e' ->
57:       IF inline_state == None
58:         IF detail_subfocus == Body -> dispatch OpenInlineEditor(IssueBody)
59:         ELSE IF detail_subfocus == Comment(i) ->
60:           IF comment is editable -> dispatch OpenInlineEditor(Comment(i))
61:           ELSE -> show non-blocking hint "Cannot edit this comment"
62:     CASE 'r' ->
63:       IF inline_state == None
64:         IF detail_subfocus == Comment(i)
65:           -> dispatch OpenReplyComposer(i)
66:         ELSE -> show non-blocking hint "Focus a comment to reply"
67:     CASE 'S' ->
68:       IF inline_state == None
69:         IF agents exist -> dispatch OpenAgentChooser
70:         ELSE -> show "No agents available" message

71: FUNCTION handle_inline_key(key_event, state, ctx)
72:   MATCH key_event
73:     CASE Esc -> dispatch InlineCancelOrEsc
74:     CASE Cmd+Enter | Ctrl+Enter -> dispatch InlineSubmit
75:     CASE Char(c) -> dispatch InlineChar(c)
76:     CASE Backspace -> dispatch InlineBackspace
77:     CASE _ -> consume (no leak to outer handlers)

78: FUNCTION handle_inline_submit(state, ctx)
79:   MATCH state.issues_state.inline_state
80:     CASE Composer { target: NewComment, text }
81:       VALIDATE text non-empty
82:       CALL gh_client.create_comment(owner, repo, issue_number, text)
83:       ON success -> dispatch CommentCreated
84:       ON failure -> dispatch CommentCreateFailed
85:     CASE Composer { target: Reply { comment_index, author }, text }
86:       VALIDATE text non-empty
87:       CALL gh_client.create_comment(owner, repo, issue_number, text)
88:       ON success -> dispatch CommentCreated
89:       ON failure -> dispatch CommentCreateFailed
90:     CASE Editor { target: IssueBody, text }
91:       CALL gh_client.update_issue_body(owner, repo, issue_number, text)
92:       ON success -> dispatch IssueBodyUpdated
93:       ON failure -> dispatch MutationFailed
94:     CASE Editor { target: Comment { comment_index }, text }
95:       GET comment_id from detail.comments[comment_index]
96:       CALL gh_client.update_comment(owner, repo, comment_id, text)
97:       ON success -> dispatch CommentUpdated
98:       ON failure -> dispatch MutationFailed
99:   SET inline_state = None

100: FUNCTION handle_agent_chooser_key(key_event, state, ctx)
101:  MATCH key_event
102:    CASE Up -> dispatch AgentChooserNavigateUp
103:    CASE Down -> dispatch AgentChooserNavigateDown
104:    CASE Enter ->
105:      GET selected_agent from chooser
106:      BUILD send_payload from current issue detail + focused comment + issue_base_prompt
107:      DELIVER payload to selected agent runtime
108:      dispatch AgentChooserConfirm
109:    CASE Esc -> dispatch AgentChooserCancel

110: FUNCTION handle_search_input_key(key_event, state, ctx)
111:  MATCH key_event
112:    CASE Enter -> dispatch ApplySearch
113:    CASE Esc ->
114:      IF search_query non-empty -> dispatch ClearSearch (keep focused)
115:      ELSE -> dispatch BlurSearchInput
116:    CASE Char(c) -> append to search_query
117:    CASE Backspace -> remove last char from search_query

118: FUNCTION handle_filter_controls_key(key_event, state, ctx)
119:  // Navigate filter form fields and apply/clear/cancel
120:  MATCH key_event
121:    CASE Tab -> next filter field
122:    CASE Shift+Tab -> prev filter field
123:    CASE Enter -> dispatch ApplyFilter; close controls
124:    CASE Esc -> dispatch CloseFilterControls (cancel, no commit)
125:    CASE Char(c) -> edit focused filter field

126: FUNCTION handle_repo_scope_change_in_issues_mode(state, new_repo_id)
127:  DISCARD in-flight requests for prior scope
128:  IF inline_state != None
129:    CLEAR inline_state
130:    SET draft_notice = "Unsent draft discarded"
131:  CLEAR issues list, detail, comments, pagination state
132:  SET list_loading = true
133:  EMIT side_effect: load_issue_list(new_repo_id, committed_filter)

134: FUNCTION compose_reply_prefill(comment)
135:  RETURN "@{comment.author_login} "

136: FUNCTION exclusivity_guard(state, requested_control)
137:  IF state.issues_state.inline_state != None
138:    RETURN Err("Another inline control is active")
139:  RETURN Ok
```
