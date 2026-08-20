// misskey.local -> 127.0.0.1 だけを差し替えるgetaddrinfo()フック。
// それ以外のホスト名は本来のgetaddrinfo()にそのまま委譲する。
#define _GNU_SOURCE
#include <netdb.h>
#include <string.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <netinet/in.h>

typedef int (*real_getaddrinfo_t)(const char *, const char *, const struct addrinfo *, struct addrinfo **);

int getaddrinfo(const char *node, const char *service, const struct addrinfo *hints, struct addrinfo **res) {
    if (node != NULL && strcmp(node, "misskey.local") == 0) {
        struct addrinfo *ai = calloc(1, sizeof(struct addrinfo));
        struct sockaddr_in *sa = calloc(1, sizeof(struct sockaddr_in));
        sa->sin_family = AF_INET;
        sa->sin_addr.s_addr = htonl(0x7f000001); // 127.0.0.1
        if (service != NULL) {
            sa->sin_port = htons((unsigned short)atoi(service));
        }
        ai->ai_family = AF_INET;
        ai->ai_socktype = hints ? hints->ai_socktype : SOCK_STREAM;
        ai->ai_protocol = hints ? hints->ai_protocol : 0;
        ai->ai_addrlen = sizeof(struct sockaddr_in);
        ai->ai_addr = (struct sockaddr *)sa;
        ai->ai_canonname = NULL;
        ai->ai_next = NULL;
        *res = ai;
        return 0;
    }
    static real_getaddrinfo_t real_getaddrinfo = NULL;
    if (real_getaddrinfo == NULL) {
        real_getaddrinfo = (real_getaddrinfo_t)dlsym(RTLD_NEXT, "getaddrinfo");
    }
    return real_getaddrinfo(node, service, hints, res);
}
