# syntax=docker/dockerfile:1.2
FROM alpine

# Install ssh client and git
RUN apk add --no-cache openssh-client git

# Download public key for github.com
RUN mkdir -p -m 0700 ~/.ssh && ssh-keyscan github.com >> ~/.ssh/known_hosts

RUN --mount=type=ssh,id=github ssh -T git@github.com >> /tmp/output.txt 2>&1 || true
