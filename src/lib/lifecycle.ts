import { onMount } from "svelte";

type Disposer = (() => void) | undefined | null;

/**
 * Run async setup on mount and dispose whatever it returns on unmount.
 *
 * `setup` performs the initial fetches/subscriptions and returns the cleanup
 * callbacks to run on teardown — typically the unlisten functions from
 * `events.*.listen`, plus any other teardown closures. Disposers are also run
 * if the component unmounts before setup resolves, avoiding leaked listeners.
 */
export function onMountAsync(setup: () => Promise<Disposer[]>): void {
    onMount(() => {
        let disposers: Disposer[] = [];
        let unmounted = false;
        void (async () => {
            disposers = await setup();
            if (unmounted) disposers.forEach((dispose) => dispose?.());
        })();
        return () => {
            unmounted = true;
            disposers.forEach((dispose) => dispose?.());
        };
    });
}
