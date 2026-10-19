/* Launch an explicitly selected SD-card test program on adbd without nohup.
 * A CLOEXEC pipe confirms exec before the parent exits; child gets its own
 * session and redirects all standard descriptors. Does not stop any process.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    if (argc < 3) {
        fprintf(stderr, "usage: %s LOG PROGRAM [ARGS...]\n", argv[0]);
        return 2;
    }
    int ready[2];
    if (pipe2(ready, O_CLOEXEC)) { perror("pipe"); return 1; }
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 1; }
    if (!pid) {
        close(ready[0]);
        if (setsid() < 0) goto failed;
        int input = open("/dev/null", O_RDONLY);
        int log = open(argv[1], O_WRONLY | O_CREAT | O_EXCL, 0600);
        if (input < 0 || log < 0 || dup2(input, STDIN_FILENO) < 0 ||
            dup2(log, STDOUT_FILENO) < 0 || dup2(log, STDERR_FILENO) < 0)
            goto failed;
        close(input);
        close(log);
        execvp(argv[2], argv + 2);
failed: {
            int saved = errno;
            ssize_t ignored = write(ready[1], &saved, sizeof(saved));
            (void)ignored;
            _exit(127);
        }
    }
    close(ready[1]);
    int error = 0;
    ssize_t size;
    do { size = read(ready[0], &error, sizeof(error)); } while (size < 0 && errno == EINTR);
    close(ready[0]);
    if (size != 0) {
        if (size > 0) errno = error;
        perror("launch");
        waitpid(pid, NULL, 0);
        return 1;
    }
    printf("%d\n", pid);
    return 0;
}
