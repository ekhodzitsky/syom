# Isolated FFmpeg 9.0.1 adapter. Not invoked by cargo test --workspace.
# make FFMPEG_PREFIX=/path/to/prefix

FFMPEG_PREFIX ?= $(error set FFMPEG_PREFIX to a FFmpeg 9.0.1 install)

CC ?= gcc
CFLAGS ?= -O2 -Wall -Wextra -std=c11
CPPFLAGS += -I$(FFMPEG_PREFIX)/include
LDFLAGS += -L$(FFMPEG_PREFIX)/lib
LDLIBS += -lavformat -lavcodec -lswresample -lavutil -lm -lpthread

.PHONY: all clean

all: avc_driver

avc_adapt.o: avc_adapt.c avc_adapt.h
	$(CC) $(CFLAGS) $(CPPFLAGS) -c avc_adapt.c -o $@

avc_driver.o: avc_driver.c avc_adapt.h
	$(CC) $(CFLAGS) $(CPPFLAGS) -c avc_driver.c -o $@

avc_driver: avc_adapt.o avc_driver.o
	$(CC) $(CFLAGS) -o $@ avc_adapt.o avc_driver.o $(LDFLAGS) $(LDLIBS)

clean:
	rm -f avc_adapt.o avc_driver.o avc_driver
