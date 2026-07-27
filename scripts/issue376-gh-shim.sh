#!/bin/sh
# Deterministic fail-closed GitHub CLI fixture for the issue 376 Changes scenario.

set -eu

print_json() {
    printf '%s\n' "$1"
}

comment_marker_path() {
    workspace=${GH_SHIM_WORKSPACE:-}
    marker=${GH_SHIM_COMMENT_MARKER:-}
    case "$workspace" in
        /*) ;;
        *) return 1 ;;
    esac
    [ "$marker" = "$workspace/comment-created" ] || return 1
    printf '%s\n' "$marker"
}

COMMENT_MARKER=$(comment_marker_path) || {
    printf '%s\n' 'issue376 gh fixture requires a contained comment marker' >&2
    exit 64
}

if [ "$#" -eq 2 ] && [ "$1" = "auth" ] && [ "$2" = "status" ]; then
    exit 0
fi

if [ "$#" -ge 2 ] && [ "$1" = "pr" ] && [ "$2" = "view" ]; then
    print_json '{"number":376,"title":"Add delta review to PR screen","state":"OPEN","mergedAt":null,"author":{"login":"contributor"},"createdAt":"2026-07-20T00:00:00Z","updatedAt":"2026-07-26T00:00:00Z","headRefName":"feature/delta-review","headRefOid":"head376","baseRefName":"main","isDraft":false,"labels":[],"assignees":[],"milestone":null,"body":"Review changes without leaving Jefe.","url":"https://github.com/fixture/project/pull/376","reviewDecision":null,"mergeable":"MERGEABLE","mergeStateStatus":"CLEAN","statusCheckRollup":[],"reviews":[{"id":"review-376","author":{"login":"contributor"},"state":"COMMENTED","submittedAt":"2026-07-26T00:00:00Z","body":""}]}'
    exit 0
fi

if [ "$#" -ge 2 ] && [ "$1" = "api" ] && [ "$2" = "graphql" ]; then
    query=''
    for argument in "$@"; do
        case "$argument" in
            query=*) query=${argument#query=} ;;
        esac
    done
    case "$query" in
        *'search(type: ISSUE'*)
            print_json '{"data":{"search":{"nodes":[{"number":376,"title":"Add delta review to PR screen","state":"OPEN","mergedAt":null,"author":{"login":"contributor"},"updatedAt":"2026-07-26T00:00:00Z","headRefName":"feature/delta-review","headRefOid":"head376","baseRefName":"main","isDraft":false,"mergeable":"MERGEABLE","reviewDecision":null,"statusCheckRollup":{"contexts":{"nodes":[]}},"assignees":{"nodes":[]},"labels":{"nodes":[]},"comments":{"totalCount":0},"body":"Review changes without leaving Jefe."}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}'
            exit 0
            ;;
        *reviewThreads*)
            if [ -f "$COMMENT_MARKER" ]; then
                # Derive the refreshed thread body from the stored submitted
                # body so the scenario proves the mutation data propagated.
                # Use POSIX read (not external cat) so a restricted PATH in the
                # test harness does not break the fixture.
                thread_body='x'
                if [ -f "${COMMENT_MARKER}.body" ]; then
                    thread_body=''
                    while IFS= read -r line || [ -n "$line" ]; do
                        if [ -n "$thread_body" ]; then
                            thread_body="${thread_body}${line}"
                        else
                            thread_body="$line"
                        fi
                    done < "${COMMENT_MARKER}.body"
                fi
                response='{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[{"id":"thread-created-376","isResolved":false,"isOutdated":false,"path":"src/app.rs","line":3,"diffSide":"RIGHT","startLine":3,"startDiffSide":"RIGHT","originalLine":3,"originalStartLine":3,"comments":{"nodes":[{"databaseId":9001,"author":{"login":"contributor"},"createdAt":"2026-07-26T00:00:00Z","lastEditedAt":null,"body":"'"$thread_body"'","pullRequestReview":{"id":"review-376"}}]}}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
                print_json "$response"
            else
                print_json '{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
            fi
            exit 0
            ;;
        *'comments(first:'*)
            print_json '{"data":{"repository":{"pullRequest":{"comments":{"nodes":[],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}}'
            exit 0
            ;;
        *'object(oid:'*)
            print_json '{"data":{"repository":{"object":{"byteSize":95,"isBinary":false,"isTruncated":false,"text":"fn render() {\n    let unchanged = true;\n    new_call();\n    assert!(unchanged);\n}\n"}}}}'
            exit 0
            ;;
    esac
fi

if [ "$#" -ge 4 ] && [ "$1" = "api" ] && [ "$2" = "--method" ] \
    && [ "$3" = "POST" ]; then
    case "$4" in
        *'/pulls/376/comments')
            # Validate the mutation argv structurally: each of body, commit_id,
            # path, line, and side must appear exactly once with the expected
            # value, proving the scenario exercised real mutation data.
            submitted_body=''
            submitted_commit_id=''
            submitted_path=''
            submitted_line=''
            submitted_side=''
            body_count=0
            commit_id_count=0
            path_count=0
            line_count=0
            side_count=0
            flag=''
            for argument in "$@"; do
                case "$flag" in
                    -f)
                        case "$argument" in
                            body=*)
                                submitted_body=${argument#body=}
                                body_count=$((body_count + 1)) ;;
                            commit_id=*)
                                submitted_commit_id=${argument#commit_id=}
                                commit_id_count=$((commit_id_count + 1)) ;;
                            path=*)
                                submitted_path=${argument#path=}
                                path_count=$((path_count + 1)) ;;
                            side=*)
                                submitted_side=${argument#side=}
                                side_count=$((side_count + 1)) ;;
                        esac
                        flag=''
                        continue
                        ;;
                    -F)
                        case "$argument" in
                            line=*)
                                submitted_line=${argument#line=}
                                line_count=$((line_count + 1)) ;;
                        esac
                        flag=''
                        continue
                        ;;
                esac
                case "$argument" in
                    -f) flag=-f ;;
                    -F) flag=-F ;;
                    *) flag='' ;;
                esac
            done

            if [ "$body_count" -ne 1 ] || [ "$commit_id_count" -ne 1 ] \
                || [ "$path_count" -ne 1 ] || [ "$line_count" -ne 1 ] \
                || [ "$side_count" -ne 1 ]; then
                printf '%s\n' 'issue376 gh fixture: POST argv must carry each field exactly once' >&2
                exit 64
            fi
            if [ "$submitted_body" != "x" ] \
                || [ "$submitted_commit_id" != "head376" ] \
                || [ "$submitted_path" != "src/app.rs" ] \
                || [ "$submitted_line" != "3" ] \
                || [ "$submitted_side" != "RIGHT" ]; then
                printf '%s\n' 'issue376 gh fixture: POST argv values do not match expected mutation' >&2
                exit 64
            fi

            # Derive the stored marker and refreshed thread body from the
            # submitted values so the scenario proves mutation data end-to-end.
            marker_text="created ${submitted_side} ${submitted_path}:${submitted_line}"
            printf '%s\n' "$marker_text" > "$COMMENT_MARKER"
            printf '%s\n' "$submitted_body" > "${COMMENT_MARKER}.body"
            response='{"id":9001,"body":"'"$submitted_body"'","path":"'"$submitted_path"'","line":'"$submitted_line"',"side":"'"$submitted_side"'","created_at":"2026-07-26T00:00:00Z","user":{"login":"contributor"}}'
            print_json "$response"
            exit 0
            ;;
    esac
fi

if [ "$#" -ge 2 ] && [ "$1" = "api" ]; then
    case "$2" in
        *'/pulls/376/files'*)
            print_json '[{"sha":"blob-modified-376","filename":"src/app.rs","status":"modified","additions":1,"deletions":1,"changes":2,"blob_url":"https://github.com/fixture/project/blob/head376/src/app.rs","raw_url":"https://github.com/fixture/project/raw/head376/src/app.rs","contents_url":"https://api.github.com/repos/fixture/project/contents/src/app.rs?ref=head376","patch":"@@ -1,3 +1,3 @@\n fn render() {\n     let unchanged = true;\n-    old_call();\n+    new_call();"},{"sha":"blob-removed-376","filename":"docs/old.md","status":"removed","additions":0,"deletions":2,"changes":2,"blob_url":"https://github.com/fixture/project/blob/base376/docs/old.md","raw_url":"https://github.com/fixture/project/raw/base376/docs/old.md","contents_url":"https://api.github.com/repos/fixture/project/contents/docs/old.md?ref=head376","patch":"@@ -1,2 +0,0 @@\n-Old documentation.\n-Remove after migration."}]'
            exit 0
            ;;
    esac
fi

printf 'issue376 gh fixture rejected argv:' >&2
for argument in "$@"; do
    printf ' <%s>' "$argument" >&2
done
printf '\n' >&2
exit 64
