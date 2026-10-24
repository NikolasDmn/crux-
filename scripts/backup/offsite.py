"""Off-site copy for scripts/backup.sh: uploads stdin to an S3-compatible bucket (Backblaze B2,
Hetzner Object Storage, AWS…), or prunes old backups. Runs inside the files container (boto3):

    docker compose exec -T -e BACKUP_S3_… files python -c "$(cat scripts/backup/offsite.py)" put <key> < file
    docker compose exec -T -e BACKUP_S3_… files python -c "$(cat scripts/backup/offsite.py)" prune <days>

Keys are `<prefix><UTC timestamp>/<file>`; prune drops whole timestamps older than <days>.
"""

import datetime as dt
import os
import sys

import boto3

s3 = boto3.client(
    "s3",
    endpoint_url=os.environ.get("BACKUP_S3_ENDPOINT") or None,
    aws_access_key_id=os.environ["BACKUP_S3_ACCESS_KEY"],
    aws_secret_access_key=os.environ["BACKUP_S3_SECRET_KEY"],
    region_name=os.environ.get("BACKUP_S3_REGION") or "us-east-1",
)
bucket = os.environ["BACKUP_S3_BUCKET"]
prefix = os.environ.get("BACKUP_S3_PREFIX", "crux/")

if sys.argv[1] == "put":
    # Multipart from the stream: dumps never sit in memory whole.
    s3.upload_fileobj(sys.stdin.buffer, bucket, prefix + sys.argv[2])
    print(f"uploaded s3://{bucket}/{prefix}{sys.argv[2]}")
elif sys.argv[1] == "prune":
    cutoff = dt.datetime.now(dt.UTC) - dt.timedelta(days=int(sys.argv[2]))
    dropped = 0
    for page in s3.get_paginator("list_objects_v2").paginate(Bucket=bucket, Prefix=prefix):
        for obj in page.get("Contents", []):
            stamp = obj["Key"][len(prefix) :].split("/", 1)[0]
            try:
                when = dt.datetime.strptime(stamp, "%Y%m%dT%H%M%SZ").replace(tzinfo=dt.UTC)
            except ValueError:
                continue  # not ours
            if when < cutoff:
                s3.delete_object(Bucket=bucket, Key=obj["Key"])
                dropped += 1
    print(f"pruned {dropped} objects older than {sys.argv[2]} days")
