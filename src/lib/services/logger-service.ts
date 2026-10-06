import { isTauri } from '@tauri-apps/api/core';
import { error as writeNativeError, info as writeNativeInfo, warn as writeNativeWarn } from '@tauri-apps/plugin-log';

type LogContext = Readonly<Record<string, unknown>>;
type LogLevel = 'info' | 'warn' | 'error';
type NativeLogWriter = (message: string) => Promise<void>;
const MAX_CONTEXT_CHARS = 8192;
const SECRET_FIELD = /password|passwd|secret|token|authorization|cookie|api[-_]?key|credential/i;

/** Preserve diagnostic context without allowing logging failures to break product actions. */
function serializeContext(context: LogContext): string {
  const seen = new WeakSet<object>();
  try {
    const serialized = JSON.stringify(context, (key: string, value: unknown): unknown => {
      if (SECRET_FIELD.test(key)) return '[redacted]';
      if (typeof value === 'bigint') return value.toString();
      if (typeof value !== 'object' || value === null) return value;
      if (seen.has(value)) return '[circular]';
      seen.add(value);
      // Error properties are non-enumerable; plain JSON.stringify otherwise records only {}.
      if (value instanceof Error) {
        return { ...value, name: value.name, message: value.message, stack: value.stack, cause: value.cause };
      }
      return value;
    });
    if (serialized.length <= MAX_CONTEXT_CHARS) return serialized;
    // JSON preserves supplementary Unicode characters as UTF-16 pairs. Splitting a pair makes
    // the outer Tauri IPC message invalid for Rust's JSON decoder and loses the entire log.
    let end = MAX_CONTEXT_CHARS;
    const last = serialized.charCodeAt(end - 1);
    const next = serialized.charCodeAt(end);
    if (last >= 0xd800 && last <= 0xdbff && next >= 0xdc00 && next <= 0xdfff) end--;
    return `${serialized.slice(0, end)}…[truncated]`;
  } catch {
    return '[context serialization failed]';
  }
}

/**
 * Central frontend logging boundary. Production code emits stable domains,
 * events, and context without depending on the current log destination.
 */
export class LoggerService {
  static info(domain: string, event: string, context?: LogContext): void {
    LoggerService.write('info', domain, event, context);
  }

  static warn(domain: string, event: string, context?: LogContext): void {
    LoggerService.write('warn', domain, event, context);
  }

  static error(domain: string, event: string, context?: LogContext): void {
    LoggerService.write('error', domain, event, context);
  }

  private static write(level: LogLevel, domain: string, event: string, context?: LogContext): void {
    const message = `[${domain}] ${event}`;
    const args: [string] | [string, LogContext] = context ? [message, context] : [message];

    LoggerService.writeNative(level, context ? `${message} context=${serializeContext(context)}` : message);

    if (level === 'error') {
      console.error(...args);
      return;
    }
    if (level === 'warn') {
      console.warn(...args);
      return;
    }
    console.info(...args);
  }

  private static writeNative(level: LogLevel, message: string): void {
    // Browser previews and unit tests intentionally retain console-only
    // logging. Native logs also retain the context needed to investigate user reports.
    if (!isTauri()) return;

    const writers: Readonly<Record<LogLevel, NativeLogWriter>> = {
      error: writeNativeError,
      info: writeNativeInfo,
      warn: writeNativeWarn,
    };
    void writers[level](message).catch(error => {
      console.warn('[frontend-logging] native_log_failed', {
        level,
        error: error instanceof Error ? error.name : typeof error,
      });
    });
  }
}
