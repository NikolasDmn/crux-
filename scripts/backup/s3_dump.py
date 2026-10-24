"""Writes every object of the Crux bucket to stdout as a tar stream (run inside the files container,
which has boto3 and the store credentials):

    docker compose exec -T files python - < scripts/backup/s3_dump.py > objects.tar
"""

import io
import os
import sys
import tarfile

import boto3

s3 = boto3.client(
    "s3",
    endpoint_url=os.environ["CRUX_S3_ENDPOINT"],
    aws_access_key_id=os.environ["CRUX_S3_ACCESS_KEY"],
    aws_secret_access_key=os.environ["CRUX_S3_SECRET_KEY"],
    region_name=os.environ.get("CRUX_S3_REGION", "us-east-1"),
)
bucket = os.environ.get("CRUX_S3_BUCKET", "crux-files")
count = 0
with tarfile.open(fileobj=sys.stdout.buffer, mode="w|") as tar:
    for page in s3.get_paginator("list_objects_v2").paginate(Bucket=bucket):
        for obj in page.get("Contents", []):
            body = s3.get_object(Bucket=bucket, Key=obj["Key"])["Body"].read()
            info = tarfile.TarInfo(name=obj["Key"])
            info.size = len(body)
            info.mtime = int(obj["LastModified"].timestamp())
            tar.addfile(info, io.BytesIO(body))
            count += 1
print(f"{count} objects", file=sys.stderr)
