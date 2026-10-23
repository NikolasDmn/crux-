"""Shared plumbing for the Crux Python services (backend-spec 2, 3).

Layering per service: app/http -> app/service -> app/domain (pure) + app/repo; app/clients for
other services, Redis and the S3 object store. This package holds what every service needs identically.
"""
