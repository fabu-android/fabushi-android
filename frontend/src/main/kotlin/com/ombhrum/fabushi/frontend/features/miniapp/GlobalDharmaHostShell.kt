package com.ombhrum.fabushi

private fun htmlEscape(value: String): String = buildString(value.length) {
    value.forEach { character ->
        append(
            when (character) {
                '&' -> "&amp;"
                '<' -> "&lt;"
                '>' -> "&gt;"
                '"' -> "&quot;"
                '\'' -> "&#39;"
                else -> character
            },
        )
    }
}

/**
 * Host-owned local document for the official Global Dharma `mcp-http` surface.
 *
 * The remote MCP server remains the only Mini App runtime. This HTML is merely the Telegram-style
 * presentation surface hosted under the Android local Mini App origin, which means the existing
 * WebMCP bridge can safely project the Fabushi account and shared Host events without exposing
 * native credentials to a remote document.
 */
internal fun globalDharmaHostShell(plugin: MarketplacePlugin): String? {
    if (plugin.pluginId != MiniAppPlatformBridge.GLOBAL_DHARMA_ID) return null
    val toolButtons = plugin.tools.joinToString("\n") { tool ->
        val approval = if (tool.approval == "none") "read" else "approval"
        """<button class="tool" data-tool="${htmlEscape(tool.name)}" data-approval="$approval"><span>${htmlEscape(tool.description)}</span><small>${htmlEscape(tool.name)}</small></button>"""
    }
    return """
        <!doctype html>
        <html lang="zh-CN">
        <head>
          <meta charset="utf-8">
          <meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
          <title>${htmlEscape(plugin.displayName)}</title>
          <style>
            :root{color-scheme:light dark;font-family:Inter,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;background:#f4f5f7;color:#17181b}
            *{box-sizing:border-box}body{margin:0;min-height:100vh;background:linear-gradient(180deg,#eef2f7 0,#f7f8fa 42%,#fff 100%)}
            main{max-width:720px;margin:0 auto;padding:18px 16px 140px}.hero{border-radius:24px;padding:20px;background:rgba(255,255,255,.92);box-shadow:0 18px 50px rgba(31,41,55,.09)}
            .eyebrow{font-size:11px;font-weight:800;letter-spacing:.14em;color:#5d77a8}.hero h1{margin:8px 0 6px;font-size:25px}.hero p{margin:0;color:#697386;line-height:1.55;font-size:14px}
            .account{display:flex;gap:8px;align-items:center;margin-top:14px;font-size:12px;color:#526079}.dot{width:8px;height:8px;border-radius:50%;background:#37b47e;box-shadow:0 0 0 5px rgba(55,180,126,.11)}
            section{margin-top:16px}.section-title{font-size:12px;font-weight:800;color:#6d7788;margin:0 0 8px 4px}.tools{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:10px}
            .tool{border:0;border-radius:18px;padding:15px;text-align:left;background:#fff;color:#1b1d21;box-shadow:0 8px 24px rgba(31,41,55,.07);min-height:82px}.tool span{display:block;font-size:14px;font-weight:700;line-height:1.35}.tool small{display:block;margin-top:8px;color:#8791a2}.tool[data-approval=approval]::after{content:"需确认";float:right;margin-top:-18px;font-size:10px;color:#b87333}
            .panel{border-radius:18px;padding:15px;background:#fff;box-shadow:0 8px 24px rgba(31,41,55,.07)}#status{font-size:13px;font-weight:700;color:#39465d}#output{white-space:pre-wrap;word-break:break-word;max-height:220px;overflow:auto;font:12px/1.55 ui-monospace,SFMono-Regular,Menlo,monospace;color:#536174;margin-top:9px}
            #events{display:grid;gap:7px}.event{border-left:3px solid #7e91b6;padding:7px 10px;background:rgba(255,255,255,.7);border-radius:0 12px 12px 0;font-size:12px}.event b{margin-right:6px}.event.failed{border-left-color:#d05d5d}.event.completed{border-left-color:#35a978}
            @media(max-width:520px){.tools{grid-template-columns:1fr 1fr}main{padding-left:12px;padding-right:12px}.hero{border-radius:20px}}
            @media(prefers-color-scheme:dark){:root{background:#101216;color:#edf0f4}body{background:linear-gradient(180deg,#121821,#0f1115 45%,#0c0d10)}.hero,.tool,.panel{background:#191d24;color:#eef2f7;box-shadow:none}.hero p,.account,.section-title,.tool small,#output{color:#9da8b9}.tool{color:#edf1f7}.event{background:#171b22}#status{color:#d9e0ea}}
          </style>
        </head>
        <body>
          <main>
            <div class="hero">
              <div class="eyebrow">FABUSHI MINI APP · ANDROID</div>
              <h1>${htmlEscape(plugin.displayName)}</h1>
              <p>${htmlEscape(plugin.description)}</p>
              <div class="account"><i class="dot"></i><span id="account">正在读取 Fabushi 登录态…</span></div>
            </div>
            <section><div class="section-title">WEBMCP TOOLS</div><div class="tools">$toolButtons</div></section>
            <section><div class="section-title">当前状态</div><div class="panel"><div id="status">等待同一 Host 事件…</div><pre id="output"></pre></div></section>
            <section><div class="section-title">Bot / Web UI 实时事件</div><div id="events"></div></section>
          </main>
          <script>
            (()=>{
              const account=document.getElementById('account');
              const status=document.getElementById('status');
              const output=document.getElementById('output');
              const events=document.getElementById('events');
              function renderAccount(){const state=window.__fabushiMiniAppHost?.account;if(!state)return;const user=state.user||{};account.textContent=state.loggedIn?('Fabushi 已登录 · '+(user.nickname||user.username||user.email||user.id||'当前账号')):'Fabushi 未登录';}
              function appendEvent(event){if(event.pluginId&&event.pluginId!=='global-dharma'&&event.miniAppId!=='global-dharma')return;const row=document.createElement('div');row.className='event '+(event.status||'');const label=event.tool||event.title||event.type||'event';row.innerHTML='<b></b><span></span>';row.querySelector('b').textContent=label;row.querySelector('span').textContent=event.message||event.detail||event.status||'';events.prepend(row);while(events.childElementCount>12)events.lastElementChild.remove();if(event.status)status.textContent=(event.tool?event.tool+' · ':'')+event.status+(event.message?' · '+event.message:'');}
              function renderResult(result){output.textContent=typeof result==='string'?result:JSON.stringify(result,null,2);}
              async function restoreSharedRuntime(){
                const tools=window.__fabushiWebMcp?.list?.()||[];
                if(!tools.some(tool=>tool&&tool.name==='status'))return;
                status.textContent='status · syncing · 正在恢复 Bot 共享状态';
                try{
                  const result=await window.__fabushiWebMcp.call('status',{});
                  renderResult(result);
                  status.textContent='status · synced · Bot / Web UI 同一共享状态';
                  window.dispatchEvent(new CustomEvent('fabushi:shared-runtime-restored',{detail:result}));
                }catch(error){
                  status.textContent='status · failed · 共享状态恢复失败';
                  output.textContent=String(error?.message||error);
                }
              }
              window.addEventListener('fabushi:miniapp-auth',renderAccount);
              window.addEventListener('fabushi:webmcp-ready',()=>{renderAccount();restoreSharedRuntime();});
              window.addEventListener('fabushi:host-event',event=>appendEvent(event.detail||{}));
              document.querySelectorAll('[data-tool]').forEach(button=>button.addEventListener('click',async()=>{const name=button.dataset.tool;button.disabled=true;status.textContent=name+' · running';output.textContent='';try{const result=await window.__fabushiWebMcp.call(name,{});renderResult(result);status.textContent=name+' · completed';}catch(error){status.textContent=name+' · failed';output.textContent=String(error?.message||error);}finally{button.disabled=false;}}));
              renderAccount();
            })();
          </script>
        </body>
        </html>
    """.trimIndent()
}
