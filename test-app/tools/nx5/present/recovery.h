/* Preserve the original image if an uncertain ioctl prevents safe restoration. */
static void preserve_backup(const void *bytes, size_t length) {
    const char *path = getenv("NX5_BACKUP_FILE");
    if (!path) path = "present-recovery.bgra";
    int fd = open(path, O_WRONLY | O_CREAT | O_EXCL, 0600);
    if (fd < 0) { perror("preserve recovery image"); return; }
    const unsigned char *p = bytes;
    while (length) {
        ssize_t n = write(fd, p, length);
        if (n <= 0) { perror("write recovery image"); break; }
        p += n; length -= (size_t)n;
    }
    close(fd);
    fprintf(stderr, "presentation state uncertain; UI must remain stopped; backup=%s remaining=%zu\n", path, length);
}
