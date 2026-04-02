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

09: FUNCTION list_issues(owner, repo, filter, cursor, page_size) -> Result<IssueListResponse, GhError>
10:   // Uses GraphQL via `gh api graphql` for cursor-based pagination.
11:   // `gh issue list` does not support cursor pagination natively.
12:   BUILD query: issues(first: page_size, after: cursor, filterBy: {...}, orderBy: {field: UPDATED_AT, direction: DESC})
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
31:   RETURN Ok(IssueListResponse { issues, cursor, has_more })

32: FUNCTION get_issue_detail(owner, repo, number) -> Result<IssueDetail, GhError>
33:   BUILD args: ["issue", "view", "--repo", "{owner}/{repo}", number, "--json", DETAIL_FIELDS]
34:   RUN `gh` subprocess
35:   IF exit code != 0
36:     RETURN Err(categorized_error)
37:   PARSE stdout as JSON -> IssueDetail
38:   RETURN Ok(detail)

39: FUNCTION list_comments(owner, repo, number, cursor, page_size) -> Result<CommentsResponse, GhError>
40:   // Uses GraphQL issue comments connection for cursor-based pagination.
41:   BUILD query: repository(owner:$owner,name:$repo){ issue(number:$number){ comments(first:$page_size, after:$cursor){ nodes{...} pageInfo{hasNextPage endCursor} } } }
42:   BUILD args: ["api", "graphql", "-f", "query=...", "-f", "owner={owner}", "-f", "repo={repo}", "-F", "number={number}", "-f", "cursor={cursor}", "-F", "page_size={page_size}"]
43:   RUN `gh` subprocess
44:   IF exit code != 0 -> RETURN Err(categorized_error)
45:   PARSE stdout as JSON -> Vec<IssueComment>
46:   EXTRACT pageInfo.hasNextPage and pageInfo.endCursor
47:   RETURN Ok(CommentsResponse { comments, cursor, has_more })

48: FUNCTION create_comment(owner, repo, number, body) -> Result<IssueComment, GhError>
49:   BUILD args using `gh api` for POST issue comment endpoint
50:   args: ["api", "--method", "POST",
51:          "/repos/{owner}/{repo}/issues/{number}/comments",
52:          "-f", "body={body}"]
53:   RUN `gh` subprocess
54:   IF exit code != 0 -> RETURN Err(categorized_error)
55:   PARSE stdout as JSON -> created comment object
56:   RETURN Ok(created_comment)

57: FUNCTION update_comment(owner, repo, comment_id, body) -> Result<(), GhError>
58:   BUILD args using `gh api` for PATCH comment endpoint
59:   args: ["api", "--method", "PATCH",
60:          "/repos/{owner}/{repo}/issues/comments/{comment_id}",
61:          "-f", "body={body}"]
62:   RUN `gh` subprocess
63:   IF exit code != 0 -> RETURN Err(categorized_error)
64:   RETURN Ok

65: FUNCTION update_issue_body(owner, repo, number, body) -> Result<(), GhError>
66:   BUILD args: ["issue", "edit", "--repo", "{owner}/{repo}", number, "--body", body]
67:   RUN `gh` subprocess
68:   IF exit code != 0 -> RETURN Err(categorized_error)
69:   RETURN Ok

70: FUNCTION build_send_payload(repo, issue_detail, focused_comment, issue_base_prompt) -> SendPayload
71:   SET payload.repository = repo.slug
72:   SET payload.issue_number = issue_detail.number
73:   SET payload.issue_title = issue_detail.title
74:   SET payload.issue_body = issue_detail.body
75:   SET payload.issue_state = issue_detail.state
76:   SET payload.issue_labels = issue_detail.labels
77:   SET payload.issue_assignees = issue_detail.assignees
78:   // external_url remains part of IssueDetail display state and is intentionally not required in send payload v1
79:   IF focused_comment is Some
80:     SET payload.focused_comment = focused_comment.body
81:     SET payload.focused_comment_author = focused_comment.author_login
82:   SET payload.issue_base_prompt = issue_base_prompt
83:   RETURN payload

84: ENUM GhError
85:   NotAuthenticated(String)
86:   NotInstalled
87:   RateLimited
88:   AccessDenied(String)
89:   ApiError(String)
90:   ParseError(String)
91:   NetworkError(String)
```
