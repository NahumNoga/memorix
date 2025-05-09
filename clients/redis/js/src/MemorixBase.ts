// deno-lint-ignore-file no-explicit-any
// eslint-disable-next-line max-classes-per-file
import { Redis } from "npm:ioredis@^5.4.1";
// @deno-types="npm:@types/callback-to-async-iterator@1.1.7"
import callbackToAsyncIteratorModule from "npm:callback-to-async-iterator@1.1.1";
import type * as types from "./types.ts";
import { hashKey } from "./utils/hashKey.ts";
import { requireValue, type Value } from "./value.ts";
const callbackToAsyncIterator = callbackToAsyncIteratorModule.default;

const strigifyPayload = (payload: unknown) => {
  return payload === undefined ? "null" : JSON.stringify(payload);
};

enum QueueType {
  FIFO = "fifo",
  LIFO = "lifo",
}

export class MemorixBase {
  private namespaceNameTree: string[];

  private redis: Redis;

  private redisSub: Redis;

  private redisTasks: Redis[];

  private subscriptionCallbacks: Map<string, ((payload: any) => void)[]>;

  constructor(
    { namespaceNameTree }: { namespaceNameTree: string[] },
    { redisUrl, ref }: { redisUrl?: Value; ref?: MemorixBase },
  ) {
    this.namespaceNameTree = namespaceNameTree;
    if (ref) {
      this.redis = ref.redis;
      this.redisSub = ref.redisSub;
      this.redisTasks = ref.redisTasks;
      this.subscriptionCallbacks = ref.subscriptionCallbacks;
    } else {
      this.redis = new Redis(requireValue(redisUrl!), { lazyConnect: true });
      this.redisSub = this.redis.duplicate();
      this.redisTasks = [];
      this.subscriptionCallbacks = new Map();
    }
  }

  async connect(): Promise<void> {
    await this.redis.connect();
    await this.redisSub.connect();

    this.redisSub.on("message", (group, payload) => {
      const callbacks = this.subscriptionCallbacks.get(group);
      if (!callbacks) {
        return;
      }
      const parsedPayload = JSON.parse(payload);
      callbacks.forEach((cb) => {
        cb(parsedPayload);
      });
    });
  }

  disconnect(): void {
    this.redis.disconnect();
    this.redisSub.disconnect();
    this.redisTasks.forEach((x) => {
      x.disconnect();
    });
  }

  protected getCacheItem<
    Key,
    Payload,
    CanGet extends boolean,
    CanSet extends boolean,
    CanDelete extends boolean,
    CanExpire extends boolean,
  >(
    identifier: string,
    options: types.CacheOptions = {},
  ): types.CacheItem<Key, Payload, CanGet, CanSet, CanDelete, CanExpire> {
    const {
      ttl_ms: ttlMsValue = { type: "string", string: { value: "0" } },
      extendOnGet: extendOnGetValue = {
        type: "string",
        string: { value: "false" },
      },
    } = options;
    const getTtlMs = () => {
      const ttlMsStr = requireValue(ttlMsValue);
      const ttlMs = Number(ttlMsStr);
      if (Number.isNaN(ttlMs)) {
        throw new Error(`Exptected ttl_ms to be a number, git "${ttlMsStr}"`);
      }
      return ttlMs;
    };
    const getExtendOnGet = () => {
      const extendOnGetStr = requireValue(extendOnGetValue);
      let extendOnGet: boolean;
      if (extendOnGetStr === "true") {
        extendOnGet = true;
      } else if (extendOnGetStr === "false") {
        extendOnGet = false;
      } else {
        throw new Error(
          `Exptected extendOnGet to be a boolean, got "${extendOnGetStr}"`,
        );
      }
      return extendOnGet;
    };

    const item = {
      hasKey: true,
      key: (key: Key | undefined) => {
        return hashKey(
          (item as any).hasKey
            ? [...this.namespaceNameTree, identifier, key]
            : [...this.namespaceNameTree, identifier],
        );
      },
      extend: async (key: Key) => {
        const ttlMs = getTtlMs();
        if (ttlMs === 0) {
          return;
        }

        const hashedKey = item.key(key);
        await this.redis.pexpire(hashedKey, ttlMs.toString());
      },
      get: async (key: Key) => {
        const extendOnGet = getExtendOnGet();
        const hashedKey = item.key(key);
        const found = await this.redis.get(hashedKey);
        if (!found) {
          return null;
        }
        if (extendOnGet) {
          await item.extend(key);
        }
        return JSON.parse(found) as Payload;
      },
      set: async (key: Key, payload: Payload) => {
        const ttlMs = getTtlMs();
        const hashedKey = item.key(key);
        const params = ttlMs !== 0 ? ["PX", ttlMs.toString()] : [];
        await this.redis.set(
          hashedKey,
          strigifyPayload(payload),
          ...(params as any),
        );
      },
      delete: async (key: Key) => {
        const hashedKey = item.key(key);
        await this.redis.del(hashedKey);
      },
      expire: async (key: Key, ttlMs: number) => {
        const hashedKey = item.key(key);
        await this.redis.pexpire(hashedKey, ttlMs.toString());
      },
    };
    return item as any;
  }

  protected getCacheItemNoKey<
    Payload,
    CanGet extends boolean,
    CanSet extends boolean,
    CanDelete extends boolean,
    CanExpire extends boolean,
  >(
    ...itemArgs: any[]
  ): types.CacheItemNoKey<Payload, CanGet, CanSet, CanDelete, CanExpire> {
    const item = (this.getCacheItem as any)(...itemArgs);
    item.hasKey = false;

    return {
      key: (...args: any[]) => item.key(undefined, ...args),
      extend: (...args: any[]) => item.extend(undefined, ...args),
      get: (...args: any[]) => item.get(undefined, ...args),
      set: (...args: any[]) => item.set(undefined, ...args),
      delete: (...args: any[]) => item.delete(undefined, ...args),
      expire: (...args: any[]) => item.expire(undefined, ...args),
    } as any;
  }

  protected getPubsubItem<
    Key,
    Payload,
    CanPublish extends boolean,
    CanSubscribe extends boolean,
  >(
    identifier: string,
  ): types.PubSubItem<Key, Payload, CanPublish, CanSubscribe> {
    const item = {
      hasKey: true,
      key: (key: Key | undefined) => {
        return hashKey(
          item.hasKey
            ? [...this.namespaceNameTree, identifier, key]
            : [...this.namespaceNameTree, identifier],
        );
      },
      publish: async (key: Key, payload: Payload) => {
        const hashedKey = item.key(key);
        const subscribersSize = await this.redis.publish(
          hashedKey,
          strigifyPayload(payload),
        );
        return { subscribersSize };
      },
      subscribe: async (key: Key, callback?: (payload: Payload) => void) => {
        const hashedKey = item.key(key);
        await this.redisSub.subscribe(hashedKey);

        if (!callback) {
          const asyncIterator = callbackToAsyncIterator<
            Payload,
            (payload: Payload) => void
          >(
            // deno-lint-ignore require-await
            async (cb) => {
              this.subscriptionCallbacks.set(hashedKey, [
                ...(this.subscriptionCallbacks.get(hashedKey) ?? []),
                cb,
              ]);
              return cb;
            },
            {
              onClose: (cb) => {
                const callbacks = this.subscriptionCallbacks.get(hashedKey);
                if (!callbacks) {
                  return;
                }
                const callbackIndex = callbacks.indexOf(cb);
                if (callbackIndex === -1) {
                  return;
                }
                this.subscriptionCallbacks.set(
                  hashedKey,
                  callbacks.filter((_, i) => i !== callbackIndex),
                );
              },
            },
          );
          return {
            asyncIterator,
            unsubscribe: async () => {
              await this.redisSub.unsubscribe(hashedKey);
              if (asyncIterator.throw) {
                try {
                  await asyncIterator.throw();
                } catch (_error) {
                  // Ignore error
                }
              }
            },
          };
        }
        this.subscriptionCallbacks.set(hashedKey, [
          ...(this.subscriptionCallbacks.get(hashedKey) ?? []),
          callback,
        ]);
        return {
          unsubscribe: async () => {
            await this.redisSub.unsubscribe(hashedKey);
            const callbacks = this.subscriptionCallbacks.get(hashedKey);
            if (!callbacks) {
              return;
            }
            const callbackIndex = callbacks.indexOf(callback);
            if (callbackIndex === -1) {
              return;
            }
            this.subscriptionCallbacks.set(
              hashedKey,
              callbacks.filter((_, i) => i !== callbackIndex),
            );
          },
        } as any;
      },
    };
    return item as any;
  }

  protected getPubsubItemNoKey<
    Payload,
    CanPublish extends boolean,
    CanSubscribe extends boolean,
  >(
    ...itemArgs: any[]
  ): types.PubSubItemNoKey<Payload, CanPublish, CanSubscribe> {
    const item = (this.getPubsubItem as any)(...itemArgs);
    item.hasKey = false;

    return {
      key: (...args: any[]) => item.key(undefined, ...args),
      publish: (...args: any[]) => item.publish(undefined, ...args),
      subscribe: (...args: any[]) => item.subscribe(undefined, ...args),
    } as any;
  }

  protected getTaskItem<
    Key,
    Payload,
    CanEnqueue extends boolean,
    CanDequeue extends boolean,
    CanEmpty extends boolean,
    CanGetLen extends boolean,
  >(
    identifier: string,
    options: types.TaskOptions = {},
  ): types.TaskItem<Key, Payload, CanEnqueue, CanDequeue, CanEmpty, CanGetLen> {
    const {
      queueType: queueTypeValue = { type: "string", string: { value: "fifo" } },
    } = options;
    const getQueueType = () => {
      const queueTypeStr = requireValue(queueTypeValue);
      let queueType: QueueType;
      if (queueTypeStr === QueueType.FIFO) {
        queueType = QueueType.FIFO;
      } else if (queueTypeStr === QueueType.LIFO) {
        queueType = QueueType.LIFO;
      } else {
        throw new Error(
          `Exptected queueType to be a on of ${
            Object.values(QueueType).join(
              ", ",
            )
          }, got "${queueTypeStr}"`,
        );
      }
      return queueType;
    };

    const item = {
      hasKey: true,
      key: (key: Key | undefined) => {
        return hashKey(
          item.hasKey
            ? [...this.namespaceNameTree, identifier, key]
            : [...this.namespaceNameTree, identifier],
        );
      },
      enqueue: async (key: Key, payload: Payload) => {
        const hashedKey = item.key(key);

        const queueSize = await this.redis.rpush(
          hashedKey,
          strigifyPayload(payload),
        );

        return {
          queueSize,
        };
      },
      dequeue: async (
        key: Key,
        callback?: types.TaskDequeueCallback<Payload>,
      ) => {
        const hashedKey = item.key(key);
        const redisClient = this.redis.duplicate();
        await redisClient.connect();
        this.redisTasks.push(redisClient);
        let isStopped = false;
        const queueType = getQueueType();

        const pop = () =>
          new Promise<{ value?: Payload }>((res, rej) => {
            redisClient[queueType === QueueType.LIFO ? "brpop" : "blpop"](
              hashedKey,
              0,
              (err, popValue) => {
                if (err) {
                  if (isStopped) {
                    res({ value: undefined });
                  } else {
                    rej(err);
                  }
                  return;
                }
                if (!popValue) {
                  return;
                }
                const [, payloadStr] = popValue;
                const payload: Payload = JSON.parse(payloadStr);

                res({ value: payload });
              },
            );
          });
        let currentPop: undefined | ReturnType<typeof pop>;

        if (callback === undefined) {
          const asyncIterator: AsyncIterableIterator<Payload> = {
            [Symbol.asyncIterator]() {
              return this;
            },
            next: async () => {
              currentPop = pop();
              const { value: payload } = await currentPop;
              if (payload === undefined) {
                return {
                  done: true,
                };
              }
              return {
                value: payload,
                done: false,
              } as any;
            },
            throw() {
              return Promise.reject(new Error("Thrown"));
            },
          };
          return {
            stop: async () => {
              isStopped = true;
              redisClient.disconnect();
              const indexToRemove = this.redisTasks.indexOf(redisClient);
              if (indexToRemove !== -1) {
                this.redisTasks.splice(indexToRemove, 1);
              }
              await currentPop;
            },
            asyncIterator,
          };
        }
        const stoppedPromise = new Promise((res) => {
          // deno-lint-ignore require-await
          const cb = async (err: any, blpop: any) => {
            if (err) {
              res(err);
            }
            if (blpop) {
              redisClient[queueType === QueueType.LIFO ? "brpop" : "blpop"](
                hashedKey,
                0,
                cb,
              );

              const [, payloadStr] = blpop;
              const payload: Payload = JSON.parse(payloadStr);
              callback!(payload);
            }
          };

          redisClient[queueType === QueueType.LIFO ? "brpop" : "blpop"](
            hashedKey,
            0,
            cb,
          );
        });

        return {
          stop: async () => {
            redisClient.disconnect();
            const indexToRemove = this.redisTasks.indexOf(redisClient);
            if (indexToRemove !== -1) {
              this.redisTasks.splice(indexToRemove, 1);
            }
            await stoppedPromise;
          },
        } as any;
      },
      empty: async (key: Key) => {
        const hashedKey = item.key(key);
        await this.redis.del(hashedKey);
      },
      getLen: async (key: Key) => {
        const hashedKey = item.key(key);
        const queueSize = await this.redis.llen(hashedKey);
        return queueSize;
      },
    };

    return item as any;
  }

  protected getTaskItemNoKey<
    Payload,
    CanEnqueue extends boolean,
    CanDequeue extends boolean,
    CanEmpty extends boolean,
    CanGetLen extends boolean,
  >(
    ...itemArgs: any[]
  ): types.TaskItemNoKey<Payload, CanEnqueue, CanDequeue, CanEmpty, CanGetLen> {
    const item = (this.getTaskItem as any)(...itemArgs);
    item.hasKey = false;

    return {
      key: (...args: any[]) => item.key(undefined, ...args),
      enqueue: (...args: any[]) => item.enqueue(undefined, ...args),
      dequeue: (...args: any[]) => item.dequeue(undefined, ...args),
      empty: (...args: any[]) => item.empty(undefined, ...args),
      getLen: (...args: any[]) => item.getLen(undefined, ...args),
    } as any;
  }
}
