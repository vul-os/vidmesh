# @vidmesh/gateway-server

The gateway backend: subscribes to relays, indexes the records it *selects*
(moderation = selection), pins blobs it serves, runs the upload/transcode
pipeline, packages HLS, and exposes the REST API the web frontend consumes.
Includes the compliance toolkit (notice/counter-notice endpoints, takedown
feed subscription, legal templates).

**Status: Phase 0 scaffold — no implementation yet.** Phase 5 fills this in.
