<script lang="ts">
    import "anasthasia/bootstrap";
    import { ToastProvider } from "anasthasia";
    import "../app.css";

    let { children } = $props();
</script>

{@render children()}
<ToastProvider />
