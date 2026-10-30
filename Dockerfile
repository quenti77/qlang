# The official image: qlang alone (a static program), run by an unprivileged user.
#
# It is built by the release workflow from the binaries of the release (no compiling here):
#   dist/linux-amd64/qlang and dist/linux-arm64/qlang
# To build it by hand: put those files there, then `docker buildx build --platform linux/amd64 .`
FROM scratch
ARG TARGETARCH
COPY dist/linux-${TARGETARCH}/qlang /usr/local/bin/qlang
# no shell and no passwd file in the image: the user is a number
USER 65532:65532
WORKDIR /app
ENV HOME=/tmp
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/qlang"]
CMD ["--help"]
