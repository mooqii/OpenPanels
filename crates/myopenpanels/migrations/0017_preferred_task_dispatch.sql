UPDATE tasks
SET dispatch_mode = 'prefer',
    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
WHERE dispatch_mode = 'only';
