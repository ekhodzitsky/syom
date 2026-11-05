#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include <ucontext.h>
#include <unistd.h>
#if !defined(__linux__) || !defined(__x86_64__)
#error This diagnostic sampler requires Linux x86_64.
#endif
static int sample_fd = -1;
static volatile sig_atomic_t delivered;
static volatile sig_atomic_t failed;
static char prefix[4096];
static void on_sample(int signal_number, siginfo_t *info, void *context) {
    (void)signal_number; (void)info;
    int saved_errno = errno;
    ucontext_t *ucontext = context;
    uint64_t rip = (uint64_t)ucontext->uc_mcontext.gregs[REG_RIP];
    if (write(sample_fd, &rip, sizeof(rip)) == (ssize_t)sizeof(rip)) delivered++;
    else failed++;
    errno = saved_errno;
}
__attribute__((constructor)) static void start_sampler(void) {
    const char *configured = getenv("SYOM_SAMPLE_PREFIX");
    if (!configured || strlen(configured) > sizeof(prefix)-32) return;
    strcpy(prefix, configured);
    char path[4096];
    snprintf(path, sizeof(path), "%s.maps", prefix);
    int maps_out = open(path, O_CREAT|O_EXCL|O_WRONLY|O_CLOEXEC, 0600);
    if (maps_out < 0) return;
    int maps_in = open("/proc/self/maps", O_RDONLY|O_CLOEXEC);
    if (maps_in < 0) { close(maps_out); return; }
    char buf[8192]; ssize_t n;
    while ((n = read(maps_in, buf, sizeof(buf))) > 0) {
        ssize_t at=0;
        while(at<n) {
            ssize_t written=write(maps_out,buf+at,(size_t)(n-at));
            if(written<=0) { close(maps_in); close(maps_out); return; }
            at+=written;
        }
    }
    close(maps_in); close(maps_out);
    snprintf(path, sizeof(path), "%s.rips", prefix);
    sample_fd = open(path, O_CREAT|O_EXCL|O_WRONLY|O_CLOEXEC|O_APPEND,0600);
    if(sample_fd<0) return;
    struct sigaction action={0};
    action.sa_sigaction=on_sample;
    action.sa_flags=SA_SIGINFO|SA_RESTART;
    sigemptyset(&action.sa_mask);
    if(sigaction(SIGPROF,&action,NULL)<0) {close(sample_fd);sample_fd=-1;return;}
    struct itimerval timer={.it_interval={.tv_usec=1000},.it_value={.tv_usec=1000}};
    if(setitimer(ITIMER_PROF,&timer,NULL)<0) {close(sample_fd);sample_fd=-1;}
}
__attribute__((destructor)) static void stop_sampler(void) {
    if(sample_fd<0) return;
    struct itimerval timer={0}; setitimer(ITIMER_PROF,&timer,NULL);
    close(sample_fd);sample_fd=-1;
    char path[4096];snprintf(path,sizeof(path),"%s.meta",prefix);
    int fd=open(path,O_CREAT|O_EXCL|O_WRONLY|O_CLOEXEC,0600);
    if(fd>=0){dprintf(fd,"interval_us=1000\ndelivered=%d\nfailed=%d\n",delivered,failed);close(fd);}
}
