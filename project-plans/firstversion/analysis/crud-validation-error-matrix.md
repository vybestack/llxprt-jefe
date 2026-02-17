# CRUD Validation and Error Matrix

Plan ID: `PLAN-20260216-FIRSTVERSION-V1`

## Repository Form Rules

| Field | Validation | Error Behavior |
|---|---|---|
| Name | required, non-empty | block submit + contextual form error |
| Base dir | required, path-expandable | block submit + path error |
| Profile | optional/defaulted policy | warning or default assignment |

## Agent Form Rules

| Field | Validation | Error Behavior |
|---|---|---|
| Name | required, non-empty | block submit + contextual form error |
| Description | optional | no hard block |
| Work dir | required, path-expandable | block submit + path error |
| Profile | required/default fallback | block or default policy per spec |
| Mode flags | valid known values | reject invalid flag with error |
| pass --continue | defaults true on create | explicit checkbox/toggle state |

## Delete Modal Rules

| Action | Required confirmation | Edge behavior |
|---|---|---|
| Delete repository | explicit confirm | handle agent ownership cascade deterministically |
| Delete agent | explicit confirm + optional workdir delete | workdir delete failure surfaces error without state corruption |

## Error Surface Contract

1. Form errors render inline in form context.
2. Runtime/persistence operation errors render in status/error channel.
3. Recoverable errors never hard-crash UI loop.

## Verification Mapping

- P10: validation/error behavior tests
- P11: implementation
- P14: regression and non-destructive checks
