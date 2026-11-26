#include <errno.h>
#include <sys/resource.h>
#include <unistd.h>

/* Own-process failure oracle. Forward unrelated resource calls unchanged. */
static int reject_nofile(int resource, struct rlimit *limit) {
    if (resource == RLIMIT_NOFILE) {
        errno = EPERM;
        return -1;
    }
    return getrlimit(resource, limit);
}

__attribute__((used, section("__DATA,__interpose")))
static const struct {
    const void *replacement;
    const void *original;
} resource_override = { (const void *)reject_nofile, (const void *)getrlimit };
