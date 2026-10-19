/* Repeat recorded native Slint MCP scroll requests on device loopback.
 * No framebuffer, input-device or SDK access. Keeps USB transport out of samples.
 */
#define _GNU_SOURCE
#include <arpa/inet.h>
#include <ctype.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>

#ifndef MCP_PORT
#define MCP_PORT 9315
#endif
#define LIMIT 16384

static double now(void) {
    struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec + t.tv_nsec / 1e9;
}
static int send_all(int fd, const char *data, size_t size) {
    while (size) {
        ssize_t n = send(fd, data, size, MSG_NOSIGNAL);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) return -1;
        data += n; size -= (size_t)n;
    }
    return 0;
}
/* 0=incomplete, 1=complete; decode chunk boundaries before checking RPC errors. */
static int chunks(const char *body, size_t size, char *out) {
    const char *p = body, *end = body + size;
    size_t used = 0;
    while (p < end) {
        const char *line = strstr(p, "\r\n");
        if (!line) return 0;
        char *number_end;
        unsigned long len = strtoul(p, &number_end, 16);
        if (number_end == p || number_end > line || len > LIMIT - used) return -1;
        p = line + 2;
        if (!len) { out[used] = 0; return 1; }
        if ((size_t)(end - p) < len + 2) return 0;
        memcpy(out + used, p, len); used += len; p += len;
        if (memcmp(p, "\r\n", 2)) return -1;
        p += 2;
    }
    return 0;
}
static int request(const char *json) {
    int fd = socket(AF_INET, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd < 0) return -1;
    struct timeval timeout = {5, 0};
    setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, sizeof(timeout));
    setsockopt(fd, SOL_SOCKET, SO_SNDTIMEO, &timeout, sizeof(timeout));
    struct sockaddr_in addr = { .sin_family = AF_INET, .sin_port = htons(MCP_PORT), .sin_addr.s_addr = htonl(INADDR_LOOPBACK) };
    char header[256], reply[LIMIT + 1], decoded[LIMIT + 1];
    int len = snprintf(header, sizeof(header), "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: %zu\r\n\r\n", strlen(json));
    if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) || send_all(fd, header, (size_t)len) || send_all(fd, json, strlen(json))) goto failed;
    size_t used = 0;
    for (;;) {
        if (used == LIMIT) goto failed;
        ssize_t n = recv(fd, reply + used, LIMIT - used, 0);
        if (n < 0 && errno == EINTR) continue;
        if (n <= 0) goto failed;
        used += (size_t)n; reply[used] = 0;
        char *body = strstr(reply, "\r\n\r\n");
        if (!body) continue;
        char *status = strchr(reply, ' ');
        if (!status || atoi(status + 1) != 200) goto failed;
        *body = 0;
        char *length = strcasestr(reply, "Content-Length:");
        char *transfer = strcasestr(reply, "Transfer-Encoding:");
        long content_length = length ? strtol(length + 15, NULL, 10) : -1;
        int chunked = transfer && strcasestr(transfer, "chunked");
        *body = '\r'; body += 4;
        size_t body_size = used - (size_t)(body - reply);
        if (chunked) {
            int complete = chunks(body, body_size, decoded);
            if (complete < 0) goto failed;
            if (!complete) continue;
        } else {
            if (content_length < 0 || content_length > LIMIT) goto failed;
            if (body_size < (size_t)content_length) continue;
            memcpy(decoded, body, (size_t)content_length); decoded[content_length] = 0;
        }
        /* These requests return small acknowledgement envelopes, not UI text. */
        char compact[LIMIT + 1]; size_t j = 0;
        for (size_t i = 0; decoded[i]; i++) if (!isspace((unsigned char)decoded[i])) compact[j++] = decoded[i];
        compact[j] = 0;
        if (!strstr(compact, "\"jsonrpc\":\"2.0\"") || !strstr(compact, "\"result\":" ) ||
            strstr(compact, "\"isError\":true") || strstr(compact, "\"error\":{")) {
            fprintf(stderr, "RPC failure: %s\n", decoded); goto failed;
        }
        close(fd); return 0;
    }
failed:
    close(fd); return -1;
}
static int load(const char *path, char *out) {
    FILE *f = fopen(path, "r");
    if (!f) return -1;
    size_t n = fread(out, 1, 2048, f);
    int extra = fgetc(f); fclose(f);
    if (!n || extra != EOF) return -1;
    out[n] = 0; return 0;
}
int main(int argc, char **argv) {
    if (argc != 4) { fprintf(stderr, "usage: %s NEGATIVE_JSON POSITIVE_JSON SECONDS\n", argv[0]); return 2; }
    int seconds = atoi(argv[3]);
    char bodies[2][2049];
    if (seconds < 1 || seconds > 60 || load(argv[1], bodies[0]) || load(argv[2], bodies[1])) return 2;
    unsigned int sent = 0;
    double start = now();
    while (now() - start < seconds) {
        int direction = (int)((now() - start) / 3) % 2;
        if (request(bodies[direction])) { perror("MCP request"); return 1; }
        sent++;
        struct timespec pause = {0, 20000000};
        while (nanosleep(&pause, &pause) && errno == EINTR) {}
    }
    struct rusage usage;
    getrusage(RUSAGE_SELF, &usage);
    long long cpu_us = (long long)(usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) * 1000000 + usage.ru_utime.tv_usec + usage.ru_stime.tv_usec;
    printf("sent=%u elapsed_seconds=%.3f cpu_us=%lld\n", sent, now() - start, cpu_us);
    return 0;
}
