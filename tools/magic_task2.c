#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/ioctl.h>

int main() {
int fd = open("/dev/magic",O_RDWR);
if (fd < 0) {
printf("Cannot open /dev/magic\n");
return 1;
}

printf("Calling ioctl 0x1337...\n");
int ret = ioctl(fd,0x1337,0);

if (ret == 0) {
printf("Success! Flag should be in dmesg.\n");
} else {
printf("Failed with error: %d\n",ret);
}

close(fd);

printf("\n=== dmesg outpu ===\n");
system("dmesg | tail -15");

return 0;
}
