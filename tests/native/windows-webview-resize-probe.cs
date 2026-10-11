// Isolate viewport propagation from mpv, native child surfaces and playback IPC.
// WebView and all geometry stay on the UI thread. A worker writes queued notes;
// event timestamps are captured before queuing, not when the worker drains them.
using System;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading.Tasks;
using System.Windows.Forms;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

sealed class ResizeProbe : Form
{
    readonly WebView2 web = new WebView2();
    readonly Stopwatch clock = Stopwatch.StartNew();
    readonly BlockingCollection<string> notes = new BlockingCollection<string>();
    readonly System.Windows.Forms.Timer heartbeat = new System.Windows.Forms.Timer();
    readonly Task logger;
    bool stopping, canClose;
    long previousTick;
    const string Page = @"<!doctype html><meta charset='utf-8'>
<style>body{margin:16px;background:#17212c;color:white;font:18px system-ui}
#area{height:40vh;background:#247b8e;border:4px solid #68c9b5}
#dimensions{height:60px}button{font:inherit;padding:10px}</style>
<h1>WebView-only resize isolation</h1><div id='area'></div>
<p id='dimensions'></p><button onclick=""chrome.webview.postMessage('close')"">Close probe</button>
<script>
const area=document.querySelector('#area');
function report(){
 const data={innerWidth,innerHeight,dpr:devicePixelRatio,
   area:area.getBoundingClientRect().width,pageMs:performance.now()};
 document.querySelector('#dimensions').textContent=JSON.stringify(data);
 chrome.webview.postMessage(data);
}
new ResizeObserver(report).observe(area);addEventListener('resize',report);report();
</script>";

    void Note(string message) { if (!notes.IsAddingCompleted) notes.Add(clock.ElapsedMilliseconds + "ms " + message); }

    ResizeProbe(string directory, string page, bool statusMessages)
    {
        logger = Task.Factory.StartNew(() => {
            using (var log = new StreamWriter(Path.Combine(directory, "webview-resize.log"))) {
                log.AutoFlush = true;
                foreach (string note in notes.GetConsumingEnumerable()) log.WriteLine(note);
            }
        }, TaskCreationOptions.LongRunning);
        Text = "VSF WebView-only Resize Probe";
        ClientSize = new Size(1600, 1360); MinimumSize = new Size(560, 560);
        AutoScaleMode = AutoScaleMode.None;
        web.Dock = DockStyle.Fill; Controls.Add(web);
        ClientSizeChanged += delegate { Note("CLIENT " + ClientSize + " WEB " + web.Bounds); };
        ResizeBegin += delegate { Note("RESIZE BEGIN"); };
        ResizeEnd += async delegate {
            Note("RESIZE END");
            if (stopping || web.CoreWebView2 == null) return;
            try {
                string result = await web.CoreWebView2.ExecuteScriptAsync("JSON.stringify({innerWidth,innerHeight,devicePixelRatio})");
                if (!stopping) Note("SCRIPT " + result);
            } catch (Exception error) { if (!stopping) Note("SCRIPT ERROR " + error.Message); }
        };
        Shown += async delegate {
            try {
                var environment = await CoreWebView2Environment.CreateAsync(null, Path.Combine(directory, "webview-data"));
                if (stopping) return;
                await web.EnsureCoreWebView2Async(environment);
                if (stopping) return;
                Note("RUNTIME " + environment.BrowserVersionString);
                web.CoreWebView2.WebMessageReceived += delegate(object sender, CoreWebView2WebMessageReceivedEventArgs e) {
                    Note("PAGE " + e.WebMessageAsJson + " CLIENT " + ClientSize);
                    if (e.WebMessageAsJson == "\"close\"") Close();
                };
                web.CoreWebView2.NewWindowRequested += delegate(object sender, CoreWebView2NewWindowRequestedEventArgs e) { e.Handled = true; };
                web.CoreWebView2.NavigateToString(page);
            } catch (Exception error) { Note("UI ERROR " + error); Environment.ExitCode = 1; Close(); }
        };
        // Optional synthetic paused status isolates repeated canvas/status work
        // from decoding, native video geometry, and the mpv worker/pipe.
        heartbeat.Interval = statusMessages ? 80 : 250;
        heartbeat.Tick += delegate {
            long now = clock.ElapsedMilliseconds;
            if (previousTick != 0 && now - previousTick >= 750) Note("UI TICK GAP " + (now - previousTick));
            previousTick = now;
            if (statusMessages && !stopping && web.CoreWebView2 != null)
                web.CoreWebView2.PostWebMessageAsJson("{\"position\":2,\"pause\":true,\"subtitle\":\"GPU preview: café — 日本語\\nFirst cue · two lines\",\"decoder\":\"no\",\"idle\":false,\"generation\":0}");
            if (now >= 180000) { Note("LIFETIME TIMER"); Close(); }
        };
        heartbeat.Start();
        FormClosing += async delegate(object sender, FormClosingEventArgs e) {
            if (canClose) return;
            e.Cancel = true;
            if (stopping) return;
            stopping = true; heartbeat.Stop();
            Note("CLOSE " + e.CloseReason); web.Dispose();
            Note("WEB DISPOSED"); notes.CompleteAdding();
            try { await logger; } catch { Environment.ExitCode = 1; }
            canClose = true; Close();
        };
    }

    [DllImport("user32.dll")] static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [STAThread]
    static void Main(string[] args)
    {
        if (args.Length < 1 || args.Length > 3 || (args.Length == 3 && args[2] != "--status")) { Environment.ExitCode = 2; return; }
        // Optional existing local page is read before starting the UI loop.
        string page = args.Length >= 2 ? File.ReadAllText(args[1]) : Page;
        string architecture = RuntimeInformation.ProcessArchitecture.ToString().ToLowerInvariant();
        CoreWebView2Environment.SetLoaderDllFolderPath(Path.Combine(args[0], architecture));
        SetProcessDpiAwarenessContext(new IntPtr(-4));
        Application.EnableVisualStyles(); Application.SetCompatibleTextRenderingDefault(false);
        Application.Run(new ResizeProbe(args[0], page, args.Length == 3));
    }
}
