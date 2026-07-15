export async function whileBusy<T>(
    setBusy: (busy: boolean) => void,
    operation: () => Promise<T>,
): Promise<T> {
    setBusy(true);
    try {
        return await operation();
    } finally {
        setBusy(false);
    }
}
