// SPDX-License-Identifier: GPL-2.0
// Simple Tetris game player for the kernel tetris module

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <termios.h>
#include <unistd.h>

#define TETRIS_DEV "/dev/tetris"
#define BUFFER_SIZE 16384
#define OUTPUT_BUFFER_SIZE (BUFFER_SIZE + 256)
#define AUTO_DROP_INTERVAL 5
#define FRAME_DELAY_US 100000

static int fd = -1;
static struct termios old_tio;
static volatile sig_atomic_t running = 1;
static int use_ansi = 0;
static int use_alt_screen = 0;
static int cursor_hidden = 0;

static ssize_t write_all(int out_fd, const void *buf, size_t len) {
  const char *p = buf;
  size_t remaining = len;

  while (remaining > 0) {
    ssize_t n = write(out_fd, p, remaining);
    if (n < 0) {
      if (errno == EINTR) {
        continue;
      }
      return -1;
    }
    if (n == 0) {
      break;
    }
    p += (size_t)n;
    remaining -= (size_t)n;
  }

  return (ssize_t)(len - remaining);
}

static void write_str(const char *s) { write_all(STDOUT_FILENO, s, strlen(s)); }

static void maybe_hide_cursor(void) {
  if (!use_ansi || cursor_hidden) {
    return;
  }
  write_str("\033[?25l");
  cursor_hidden = 1;
}

void cleanup() {
  if (fd >= 0) {
    close(fd);
    fd = -1;
  }

  tcsetattr(STDIN_FILENO, TCSANOW, &old_tio);

  if (use_ansi) {
    if (use_alt_screen) {
      write_str("\033[?1049l");
    }
    write_str("\033[?25h\033[0m\033[2J\033[H");
  }
}

void signal_handler(int signum) {
  (void)signum;
  running = 0;
}

void set_nonblocking_mode(struct termios *old_tio) {
  struct termios new_tio;
  if (tcgetattr(STDIN_FILENO, old_tio) != 0) {
    perror("Failed to get terminal attributes");
    exit(EXIT_FAILURE);
  }
  new_tio = *old_tio;
  new_tio.c_lflag &= ~(ICANON | ECHO);
  new_tio.c_cc[VMIN] = 0;
  new_tio.c_cc[VTIME] = 0;
  if (tcsetattr(STDIN_FILENO, TCSANOW, &new_tio) != 0) {
    perror("Failed to set terminal attributes");
    exit(EXIT_FAILURE);
  }
}

int is_valid_command(char cmd) {
  return cmd == 'a' || cmd == 'A' || cmd == 'd' || cmd == 'D' || cmd == 's' ||
         cmd == 'S' || cmd == 'w' || cmd == 'W' || cmd == ' ' || cmd == 'r' ||
         cmd == 'R';
}

static void get_term_env(int *likely_qemu_console, int *likely_linux_console) {
  const char *term = getenv("TERM");

  *likely_qemu_console = 0;
  *likely_linux_console = 0;

  if (!term) {
    return;
  }

  /* QEMU serial console commonly uses TERM=vt100 or TERM=ansi. */
  if (strcmp(term, "vt100") == 0 || strcmp(term, "ansi") == 0) {
    *likely_qemu_console = 1;
  }

  /* Linux console (not xterm) sometimes uses TERM=linux. */
  if (strcmp(term, "linux") == 0) {
    *likely_linux_console = 1;
  }
}

static void print_controls(void) {
  int likely_qemu_console = 0;
  int likely_linux_console = 0;

  get_term_env(&likely_qemu_console, &likely_linux_console);
  (void)likely_linux_console;

  if (use_ansi) {
    /* Default: avoid alternate screen on QEMU-ish consoles. */
    use_alt_screen = !likely_qemu_console;

    if (use_alt_screen) {
      write_str("\033[?1049h");
    }
    maybe_hide_cursor();
    write_str("\033[2J\033[H");
  }

  write_str("=== Kernel Tetris Game ===\n\n");
  write_str("Controls:\n");
  write_str("  a/A - Move left\n");
  write_str("  d/D - Move right\n");
  write_str("  s/S - Move down\n");
  write_str("  w/W - Rotate\n");
  write_str("  Space - Hard drop\n");
  write_str("  r/R - Reset game\n");
  write_str("  q/Q - Quit\n\n");

  if (use_ansi) {
    write_str("TERM=");
    write_str(getenv("TERM") ? getenv("TERM") : "(unset)");
    write_str("\n");
    write_str("ANSI rendering enabled. Alt screen: ");
    write_str(use_alt_screen ? "on" : "off");
    write_str("\n\n");
  }

  write_str("Press any key to start...\n");
  (void)getchar();

  if (use_ansi) {
    write_str("\033[2J\033[H");
  }
}

void handle_input(char cmd) {
  if (!is_valid_command(cmd)) {
    return;
  }

  ssize_t written = write(fd, &cmd, 1);
  if (written != 1) {
    if (written < 0) {
      perror("Write error");
    }
    running = 0;
  }
}

static void render_game(char *buffer, int len) {
  if (!use_ansi) {
    write_all(STDOUT_FILENO, buffer, (size_t)len);
    write_all(STDOUT_FILENO, "\n", 1);
    return;
  }

  maybe_hide_cursor();

  /*
   * QEMU serial console (-nographic) flickers with ESC[J or alt-screen.
   * Minimal flicker approach:
   * - cursor home only (ESC[H)
   * - write fixed-size content (game board is constant size)
   * - pad footer line with spaces to full width (overwrite leftovers)
   * - NO clear sequences (ESC[2J, ESC[J)
   */
  char output[OUTPUT_BUFFER_SIZE];
  int pos = 0;

  pos += snprintf(output + pos, sizeof(output) - (size_t)pos, "\033[H");

  if (pos < 0 || (size_t)pos >= sizeof(output)) {
    return;
  }

  if ((size_t)len > sizeof(output) - (size_t)pos - 128) {
    len = (int)(sizeof(output) - (size_t)pos - 128);
  }

  memcpy(output + pos, buffer, (size_t)len);
  pos += len;

  /*
   * Pad the footer with spaces to fill a typical 80-char line,
   * ensuring old text doesn't remain visible.
   */
  const char *footer = "\nPress 'q' to quit";
  int footer_len = (int)strlen(footer);
  pos += snprintf(output + pos, sizeof(output) - (size_t)pos, "%s", footer);

  /* Pad to 80 chars then newline, overwriting old content. */
  int pad_count = 80 - footer_len;
  if (pad_count > 0 && (size_t)pos + (size_t)pad_count + 2 < sizeof(output)) {
    memset(output + pos, ' ', (size_t)pad_count);
    pos += pad_count;
    output[pos++] = '\n';
  }

  if (pos < 0) {
    return;
  }

  (void)write_all(STDOUT_FILENO, output, (size_t)pos);
}

int main() {
  use_ansi = isatty(STDOUT_FILENO) && isatty(STDIN_FILENO);

  fd = open(TETRIS_DEV, O_RDWR);
  if (fd < 0) {
    fprintf(stderr, "Failed to open %s: %s\n", TETRIS_DEV, strerror(errno));
    fprintf(stderr, "\nPlease load the kernel module first:\n");
    fprintf(stderr, "  insmod /lib/modules/woc2026_hello_from_skm.ko\n\n");
    return EXIT_FAILURE;
  }

  set_nonblocking_mode(&old_tio);

  signal(SIGINT, signal_handler);
  signal(SIGTERM, signal_handler);
  atexit(cleanup);

  /* Keep stdio from buffering/stalling in QEMU consoles. */
  setvbuf(stdout, NULL, _IONBF, 0);
  setvbuf(stderr, NULL, _IONBF, 0);

  print_controls();

  char buffer[BUFFER_SIZE];
  char cmd;
  int auto_drop_counter = 0;

  while (running) {
    ssize_t bytes = read(fd, buffer, sizeof(buffer) - 1);
    if (bytes > 0) {
      buffer[bytes] = '\0';
      render_game(buffer, bytes);
    } else if (bytes < 0) {
      if (errno != EAGAIN && errno != EWOULDBLOCK) {
        perror("Read error");
        break;
      }
    }

    if (read(STDIN_FILENO, &cmd, 1) > 0) {
      if (cmd == 'q' || cmd == 'Q') {
        break;
      }
      handle_input(cmd);
    }

    auto_drop_counter++;
    if (auto_drop_counter >= AUTO_DROP_INTERVAL) {
      auto_drop_counter = 0;
      cmd = 's';
      handle_input(cmd);
    }

    usleep(FRAME_DELAY_US);
  }

  cleanup();
  return EXIT_SUCCESS;
}
