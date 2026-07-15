import { onMount } from "svelte";

type Disposer = () => void;
type RegisterDisposer = (disposer: Disposer) => void;
type AsyncMountErrorHandler = (error: unknown) => void;

/**
 * Run async setup on mount and dispose registered resources on unmount.
 */
export function onMountAsync(
    setup: (onCleanup: RegisterDisposer) => Promise<void>,
    onError: AsyncMountErrorHandler = (error) => console.error("Async mount setup failed", error),
): void {
    onMount(() => {
        let disposers: Disposer[] = [];
        let unmounted = false;
        const onCleanup: RegisterDisposer = (disposer) => {
            if (unmounted) disposer();
            else disposers.push(disposer);
        };
        void setup(onCleanup).catch(onError);
        return () => {
            unmounted = true;
            disposers.forEach((dispose) => dispose());
            disposers = [];
        };
    });
}
