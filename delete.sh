#!/bin/bash

sql="
DELETE FROM file_handle
WHERE file_id IN (
    SELECT file_id
    FROM file_state
    WHERE name = '$1'
);
DELETE FROM file_state
WHERE name = '$1';
"

limbo data.db "$sql"

fd $1 . -x rm -f {}
