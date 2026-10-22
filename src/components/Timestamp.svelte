<script lang="ts">
 export let value:number;
 $:date=new Date(value);
 $:today=new Date();
 $:label=date.toDateString()===today.toDateString()
  ? date.toLocaleTimeString('en-GB',{hour:'2-digit',minute:'2-digit'})
  : [date.getFullYear(),String(date.getMonth()+1).padStart(2,'0'),String(date.getDate()).padStart(2,'0')].join('-');
</script>

<time datetime={date.toISOString()} title={date.toLocaleString()}>{label}</time>

<style>
 time{color:var(--color-muted);font-size:12px;font-weight:400;white-space:nowrap;flex-shrink:0;font-variant-numeric:tabular-nums}
 :global(.history-row) time{margin-inline:10px}
</style>
