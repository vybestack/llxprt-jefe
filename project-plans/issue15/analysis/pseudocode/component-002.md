# Component 002 Pseudocode — GitHub Client Boundary (gh CLI)

Plan ID: `PLAN-20260329-ISSUES-MODE`

Requirements: REQ-ISS-006,007,008,009,010,013,014

```text
01: STRUCT GhClient
02:   // Wraps `gh` CLI subprocess calls for GitHub API operations.
03:   // All methods are synchronous (no async); caller manages threading.

04: FUNCTION check_auth() -> Result<(), GhError>
05:   RUN `gh auth status` as subprocess
06:   IF exit code != 0
07:     RETURN Err(GhError::NotAuthenticated(stderr))
08:   RETURN Ok

09: FUNCTION list_issues(owner, repo, filter, end_cursor, page_size) -> Result<IssueListResponse, GhError>
10:   // Uses GraphQL via `gh api graphql` for cursor-based pagination.
11:   // `gh issue list` does not support cursor pagination natively.
12:   BUILD query: issues(first: page_size, after: end_cursor, filterBy: {...}, orderBy: {field: UPDATED_AT, direction: DESC})
13:   SET filterBy fields from filter:
14:     IF filter.state is Some -> states: [filter.state]
15:     IF filter.labels non-empty -> labels: filter.labels
16:     IF filter.assignee non-empty -> assignee: filter.assignee
17:     IF filter.mentioned non-empty -> mentioned: filter.mentioned
18:   IF filter.query_text non-empty
19:     // For text search, use `gh search issues` or REST search endpoint instead
20:     BUILD search args: ["search", "issues", "--repo", "{owner}/{repo}", "--json", FIELDS, "--limit", page_size]
21:     APPEND filter criteria as flags (--state, --label, etc.)
22:     APPEND filter.query_text as positional search term
23:   ELSE
24:     BUILD args: ["api", "graphql", "-f", "query=...", "-f", "variables=..."]
25:   RUN `gh` subprocess with args
26:   IF exit code != 0
27:     PARSE stderr for rate limit / auth / access errors
28:     RETURN Err(GhError::ApiError(categorized_error))
29:   PARSE response -> Vec<Issue>
30:   EXTRACT pageInfo.hasNextPage and pageInfo.endCursor from GraphQL response
31:   RETURN Ok(IssueListResponse { issues, end_cursor, has_next_page })

32: FUNCTION get_issue_detail(owner, repo, number) -> Result<IssueDetail, GhError>
33:   BUILD args: ["issue", "view", "--repo", "{owner}/{repo}", number, "--json", DETAIL_FIELDS]
34:   RUN `gh` subprocess
35:   IF exit code != 0
36:     RETURN Err(categorized_error)
37:   PARSE stdout as JSON -> IssueDetail
38:   RETURN Ok(detail)

39: FUNCTION list_comments(owner, repo, number, page, page_size) -> Result<CommentsResponse, GhError>
40:   // Uses REST API via `gh api` with page-number pagination (comments API uses page/per_page).
41:   BUILD args: ["api", "/repos/{owner}/{repo}/issues/{number}/comments",
42:          "-H", "Accept: application/vnd.github+json",
43:          "--jq", JQ_FILTER]
44:   APPEND "per_page={page_size}" query param
45:   IF page > 1 -> APPEND "page={page}" query param
46:   RUN `gh` subprocess
47:   IF exit code != 0 -> RETURN Err(categorized_error)
48:   PARSE stdout as JSON -> Vec<IssueComment>
49:   SET has_more = result count == page_size
50:   RETURN Ok(CommentsResponse { comments, next_page: page + 1, has_more })

51: FUNCTION create_comment(owner, repo, number, body) -> Result<IssueComment, GhError>
52:   BUILD args: ["issue", "comment", "--repo", "{owner}/{repo}", number, "--body", body]
53:   RUN `gh` subprocess
54:   IF exit code != 0 -> RETURN Err(categorized_error)
55:   RETURN Ok(parse_created_comment)

56: FUNCTION update_comment(owner, repo, comment_id, body) -> Result<(), GhError>
57:   BUILD args using `gh api` for PATCH comment endpoint
58:   args: ["api", "--method", "PATCH",
59:          "/repos/{owner}/{repo}/issues/comments/{comment_id}",
60:          "-f", "body={body}"]
61:   RUN `gh` subprocess
62:   IF exit code != 0 -> RETURN Err(categorized_error)
63:   RETURN Ok

64: FUNCTION update_issue_body(owner, repo, number, body) -> Result<(), GhError>
65:   BUILD args: ["issue", "edit", "--repo", "{owner}/{repo}", number, "--body", body]
66:   RUN `gh` subprocess
67:   IF exit code != 0 -> RETURN Err(categorized_error)
68:   RETURN Ok

69: FUNCTION build_send_payload(repo, issue_detail, focused_comment, issue_base_prompt) -> SendPayload
70:   SET payload.repository = repo.slug
71:   SET payload.issue_number = issue_detail.number
72:   SET payload.issue_title = issue_detail.title
73:   SET payload.issue_body = issue_detail.body
74:   SET payload.issue_state = issue_detail.state
75:   SET payload.issue_labels = issue_detail.labels
76:   SET payload.issue_assignees = issue_detail.assignees
77:   IF focused_comment is Some
78:     SET payload.focused_comment = focused_comment.body
79:     SET payload.focused_comment_author = focused_comment.author_login
80:   SET payload.issue_base_prompt = issue_base_prompt
81:   RETURN payload

82: ENUM GhError
83:   NotAuthenticated(String)
84:   NotInstalled
85:   RateLimited
86:   AccessDenied(String)
87:   ApiError(String)
88:   ParseError(String)
89:   NetworkError(String)
```
