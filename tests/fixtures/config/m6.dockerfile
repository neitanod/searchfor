ARG BASE=alpine
FROM ${BASE} AS build
RUN echo start \
 # continuation comment
 needle-build --release
RUN <<EOF
needle heredoc
EOF
FROM scratch
COPY --from=build /needle /app
