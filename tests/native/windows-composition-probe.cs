// Isolated Windows feasibility harness, not a VSF application adapter.
// UI thread owns HWNDs/WebView2. One worker owns mpv, its pipe and log IO.
// No decoded frames cross IPC. Closing keeps HWNDs alive until worker cleanup.
using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Web.Script.Serialization;
using System.Windows.Forms;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

sealed class Probe : Form
{
    sealed class Request { public string Action; public double Seconds; public int Generation; }
    sealed class PlaybackState {
        public object position, pause, subtitle, decoder, idle;
        public int generation;
    }
    readonly WebView2 web = new WebView2();
    readonly Control webHost;
    // Optional comparison path: controller and HWND geometry are UI-thread-owned.
    CoreWebView2Controller controller;
    CoreWebView2 core;
    readonly bool directWebView;
    readonly Panel clip = new Panel(), video = new Panel();
    readonly BlockingCollection<Request> requests = new BlockingCollection<Request>(64);
    readonly ConcurrentQueue<string> notes = new ConcurrentQueue<string>();
    readonly JavaScriptSerializer json = new JavaScriptSerializer();
    readonly string directory, mpvPath, html;
    readonly System.Windows.Forms.Timer lifetime = new System.Windows.Forms.Timer();
    Task worker;
    volatile bool stopping;
    bool canClose;
    readonly bool smoke;
    bool smokeDone; // Worker-owned; read after worker completion.
    int generation, editCount;
    Rectangle lastPageVideo;
    static string hostArchitecture;

    [DllImport("user32.dll")] static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr window);

    Probe(string folder, string player, string page, bool runSmoke, bool direct)
    {
        directory = folder; mpvPath = player; html = page; smoke = runSmoke;
        directWebView = direct;
        webHost = direct ? (Control)new Panel() : web;
        Text = "VSF Windows Composition Probe";
        ClientSize = new Size(1600, 1360); MinimumSize = new Size(560, 560);
        AutoScaleMode = AutoScaleMode.None;
        webHost.Dock = DockStyle.Fill; Controls.Add(webHost);
        clip.BackColor = Color.Black; video.BackColor = Color.Black;
        clip.Controls.Add(video); Controls.Add(clip); clip.BringToFront();
        ClientSizeChanged += delegate {
            notes.Enqueue("CLIENT SIZE " + ClientSize + " WEB " + webHost.Bounds + " DOCK " + webHost.Dock);
        };
        webHost.SizeChanged += delegate {
            notes.Enqueue("WEB SIZE " + webHost.Size);
            if (controller != null && !stopping) {
                controller.Bounds = webHost.ClientRectangle;
                notes.Enqueue("CONTROLLER BOUNDS " + controller.Bounds);
            }
        };
        LocationChanged += delegate {
            if (controller != null && !stopping) controller.NotifyParentWindowPositionChanged();
        };
        ResizeEnd += async delegate {
            if (core == null || stopping) return;
            try {
                string dimensions = await core.ExecuteScriptAsync(
                    "JSON.stringify({innerWidth,innerHeight,devicePixelRatio,rootWidth:document.documentElement.clientWidth,bodyWidth:document.body.clientWidth,videoWidth:document.querySelector('#video').clientWidth})");
                notes.Enqueue("PAGE DIMENSIONS " + dimensions);
            } catch (Exception error) { notes.Enqueue("PAGE DIAGNOSTIC ERROR " + error.Message); }
        };
        Shown += async delegate {
            try {
                uint window = unchecked((uint)video.Handle.ToInt64());
                worker = Task.Factory.StartNew(() => RunPlayer(window), TaskCreationOptions.LongRunning);
                var environment = await CoreWebView2Environment.CreateAsync(null, Path.Combine(directory, "webview-data"));
                if (directWebView) {
                    controller = await environment.CreateCoreWebView2ControllerAsync(webHost.Handle);
                    if (stopping) { controller.Close(); return; }
                    controller.Bounds = webHost.ClientRectangle;
                    controller.IsVisible = true;
                    core = controller.CoreWebView2;
                } else {
                    await web.EnsureCoreWebView2Async(environment);
                    if (stopping) return;
                    core = web.CoreWebView2;
                }
                notes.Enqueue("WEB HOST " + (directWebView ? "explicit controller" : "WinForms wrapper"));
                core.Settings.AreDefaultContextMenusEnabled = false;
                core.Settings.AreDevToolsEnabled = false;
                core.WebMessageReceived += Message;
                // NavigateToString reports a data URI in NavigationStarting even
                // though the resulting document's location is about:blank.
                string documentUri = "data:text/html;charset=utf-8;base64," + Convert.ToBase64String(Encoding.UTF8.GetBytes(html));
                core.NavigationStarting += delegate(object sender, CoreWebView2NavigationStartingEventArgs e) {
                    notes.Enqueue("NAVIGATION " + e.Uri.Substring(0, Math.Min(e.Uri.Length, 160)));
                    if (e.Uri != documentUri) e.Cancel = true;
                };
                core.NavigationCompleted += delegate(object sender, CoreWebView2NavigationCompletedEventArgs e) {
                    notes.Enqueue("NAVIGATION COMPLETE success=" + e.IsSuccess + " error=" + e.WebErrorStatus + " bounds=" + webHost.Bounds);
                    if (controller != null && !stopping) controller.MoveFocus(CoreWebView2MoveFocusReason.Programmatic);
                };
                core.NewWindowRequested += delegate(object sender, CoreWebView2NewWindowRequestedEventArgs e) { e.Handled = true; };
                notes.Enqueue("WebView2 " + environment.BrowserVersionString + " DPI " + GetDpiForWindow(Handle));
                core.NavigateToString(html);
            } catch (Exception error) { notes.Enqueue("UI ERROR " + error); Environment.ExitCode = 1; Close(); }
        };
        FormClosing += async delegate(object sender, FormClosingEventArgs e) {
            if (canClose) return;
            e.Cancel = true;
            if (stopping) return;
            notes.Enqueue("FORM CLOSING " + e.CloseReason);
            stopping = true; lifetime.Stop(); clip.Visible = false;
            if (worker != null) await worker;
            if (controller != null) controller.Close();
            web.Dispose(); webHost.Dispose(); canClose = true; Close();
        };
        lifetime.Interval = 300000;
        lifetime.Tick += delegate { notes.Enqueue("CLOSE SOURCE lifetime timer"); Close(); };
        lifetime.Start();
    }

    Rectangle RectangleFrom(Dictionary<string, object> value, double scale)
    {
        return Rectangle.FromLTRB((int)Math.Round(Convert.ToDouble(value["x"]) * scale),
            (int)Math.Round(Convert.ToDouble(value["y"]) * scale),
            (int)Math.Round((Convert.ToDouble(value["x"]) + Convert.ToDouble(value["width"])) * scale),
            (int)Math.Round((Convert.ToDouble(value["y"]) + Convert.ToDouble(value["height"])) * scale));
    }

    void Message(object sender, CoreWebView2WebMessageReceivedEventArgs args)
    {
        try {
            var data = json.Deserialize<Dictionary<string, object>>(args.WebMessageAsJson);
            string action = (string)data["action"];
            if (action == "bounds") {
                double scale = GetDpiForWindow(Handle) / 96.0 * (controller != null ? controller.ZoomFactor : web.ZoomFactor);
                Rectangle full = RectangleFrom((Dictionary<string, object>)data["video"], scale);
                if (full != lastPageVideo) {
                    lastPageVideo = full;
                    notes.Enqueue("PAGE VIDEO " + full + " CLIENT " + ClientSize);
                }
                Rectangle visible = Rectangle.Intersect(ClientRectangle,
                    RectangleFrom((Dictionary<string, object>)data["clip"], scale));
                // Measure on the UI thread: worker log timestamps are queue-drain
                // times and cannot identify a slow synchronous window operation.
                var layoutTime = Stopwatch.StartNew();
                string layoutStarted = DateTime.UtcNow.ToString("O");
                clip.Bounds = visible;
                long clipMilliseconds = layoutTime.ElapsedMilliseconds;
                video.Bounds = new Rectangle(full.X - visible.X, full.Y - visible.Y, full.Width, full.Height);
                long videoMilliseconds = layoutTime.ElapsedMilliseconds - clipMilliseconds;
                clip.Visible = !(bool)data["hidden"] && !stopping && visible.Width > 0 && visible.Height > 0;
                clip.BringToFront();
                if (layoutTime.ElapsedMilliseconds >= 50)
                    notes.Enqueue("SLOW LAYOUT started=" + layoutStarted + " clipMs=" + clipMilliseconds +
                        " videoMs=" + videoMilliseconds + " totalMs=" + layoutTime.ElapsedMilliseconds);
                return;
            }
            if (action == "close") { notes.Enqueue("CLOSE SOURCE web control"); Close(); return; }
            double seconds = data.ContainsKey("seconds") ? Convert.ToDouble(data["seconds"]) : 0;
            QueueRequest(action, seconds);
        } catch (Exception error) { notes.Enqueue("MESSAGE ERROR " + error.Message); }
    }

    void QueueRequest(string action, double seconds)
    {
        if (action == "reload" || action == "unload") Interlocked.Increment(ref generation);
        if (!requests.TryAdd(new Request { Action = action, Seconds = seconds, Generation = generation }))
            throw new InvalidOperationException("Command queue full");
    }

    // Exercises the media request boundary, not mouse/keyboard routing or pixels.
    bool AdvanceSmoke(PlaybackState state, ref int stage, Stopwatch since, Action<string> record)
    {
        if (!smoke || since.ElapsedMilliseconds < 2000) return false;
        bool hasPosition = state.position != null;
        double position = hasPosition ? Convert.ToDouble(state.position) : 0;
        bool paused = state.pause != null && Convert.ToBoolean(state.pause);
        bool idle = state.idle != null && Convert.ToBoolean(state.idle);
        string subtitle = state.subtitle as string ?? "";
        bool[] valid = {
            hasPosition && paused && Math.Abs(position - 2) < .05 && subtitle.Contains("café"),
            hasPosition && paused && Math.Abs(position - 2.7) < .05 && subtitle.Length == 0,
            hasPosition && paused && Math.Abs(position - 4) < .05 && subtitle.Contains("Second cue"),
            hasPosition && paused && Math.Abs(position - 4) < .05 && subtitle.Contains("Edited preview"),
            hasPosition && !paused && Math.Abs(position - 4) > .15,
            hasPosition && paused,
            idle && !hasPosition && subtitle.Length == 0,
            !idle && hasPosition && paused && Math.Abs(position - 2) < .05 && subtitle.Contains("café") && !subtitle.Contains("Edited preview"),
        };
        if (!valid[stage]) {
            if (since.ElapsedMilliseconds > 10000)
                throw new InvalidOperationException("SMOKE FAIL stage=" + stage + " state=" + new JavaScriptSerializer().Serialize(state));
            return false;
        }
        record("SMOKE PASS stage=" + stage + " state=" + new JavaScriptSerializer().Serialize(state));
        if (stage == 7) { smokeDone = true; return true; }
        string[] next = { "seek", "seek4", "edit", "toggle", "toggle", "unload", "reload" };
        QueueRequest(next[stage++], 2.7);
        since.Restart();
        return false;
    }

    void Publish(object state, int epoch)
    {
        if (stopping) return;
        BeginInvoke((Action)delegate {
            if (!stopping && epoch == generation && core != null)
                core.PostWebMessageAsJson(json.Serialize(state));
        });
    }

    static string Quote(string path) { return "\"" + path.Replace("\"", "") + "\""; }

    void RunPlayer(uint window)
    {
        var serializer = new JavaScriptSerializer();
        string pipeName = "vsf-composition-" + Guid.NewGuid().ToString("N");
        Process process = null;
        NamedPipeClientStream pipe = null;
        using (var log = new StreamWriter(Path.Combine(directory, "host.log"), false, Encoding.UTF8)) {
            log.AutoFlush = true;
            Action<string> record = line => log.WriteLine(DateTime.UtcNow.ToString("O") + " " + line);
            try {
                string media = Path.Combine(directory, "composition.mp4");
                string subtitles = Path.Combine(directory, "preview.srt");
                process = Process.Start(new ProcessStartInfo(mpvPath,
                    "--no-config --idle=yes --keep-open=yes --pause --start=2 --loop-file=inf --mute=yes --vo=gpu --gpu-api=d3d11 --gpu-context=d3d11 --hwdec=auto-copy --wid=" + window +
                    " --input-ipc-server=" + pipeName + " --msg-level=all=v --log-file=" + Quote(Path.Combine(directory, "mpv.log")) +
                    " --sub-file=" + Quote(subtitles) + " " + Quote(media)) { UseShellExecute = false, CreateNoWindow = true });
                record("START PID " + process.Id + " HWND " + window + " HOST_ARCH " + hostArchitecture);
                pipe = new NamedPipeClientStream(".", pipeName, PipeDirection.InOut, PipeOptions.Asynchronous);
                pipe.Connect(10000);
                var reader = new StreamReader(pipe, Encoding.UTF8, false, 4096, true);
                var writer = new StreamWriter(pipe, new UTF8Encoding(false), 4096, true) { AutoFlush = true };
                int requestId = 0;
                int smokeStage = 0;
                var smokeSince = Stopwatch.StartNew();
                Func<object[], bool, object> command = (arguments, optional) => {
                    int id = ++requestId;
                    writer.WriteLine(serializer.Serialize(new { command = arguments, request_id = id }));
                    var deadline = Stopwatch.StartNew();
                    while (deadline.ElapsedMilliseconds < 2000) {
                        var pending = reader.ReadLineAsync();
                        if (!pending.Wait(Math.Max(1, 2000 - (int)deadline.ElapsedMilliseconds)))
                            throw new TimeoutException("mpv IPC read timeout");
                        if (pending.Result == null) throw new IOException("mpv closed IPC");
                        var reply = serializer.Deserialize<Dictionary<string, object>>(pending.Result);
                        if (!reply.ContainsKey("request_id") || Convert.ToInt32(reply["request_id"]) != id) {
                            record("EVENT " + pending.Result); continue;
                        }
                        if ((string)reply["error"] != "success") {
                            if (!optional) throw new IOException(pending.Result);
                            return null;
                        }
                        return reply.ContainsKey("data") ? reply["data"] : null;
                    }
                    throw new TimeoutException("mpv IPC response timeout");
                };
                while (!stopping && !process.HasExited) {
                    string note;
                    while (notes.TryDequeue(out note)) record(note);
                    Request request;
                    if (requests.TryTake(out request, 80)) {
                        if (request.Generation != generation) { record("STALE " + request.Action); continue; }
                        record("COMMAND " + request.Action + " generation=" + request.Generation);
                        switch (request.Action) {
                        case "toggle": command(new object[] { "cycle", "pause" }, false); break;
                        case "seek0": case "seek2": case "seek4":
                            command(new object[] { "seek", request.Action.Substring(4), "absolute+exact" }, false); break;
                        case "seek": command(new object[] { "seek", request.Seconds, "absolute+exact" }, false); break;
                        case "unload": command(new object[] { "stop" }, false); break;
                        case "reload": command(new object[] { "loadfile", media, "replace" }, false); break;
                        case "edit":
                            string text = "1\n00:00:00,000 --> 00:00:06,000\nEdited preview " + (++editCount) + ": café — 日本語\nSecond line\n";
                            command(new object[] { "sub-add", "memory://" + text, "select" }, false);
                            var list = command(new object[] { "get_property", "track-list" }, false) as System.Collections.IEnumerable;
                            if (list != null) foreach (Dictionary<string, object> track in list)
                                if ((string)track["type"] == "sub" && track.ContainsKey("external") && (bool)track["external"] &&
                                    (!track.ContainsKey("selected") || !(bool)track["selected"]))
                                    command(new object[] { "sub-remove", track["id"] }, false);
                            break;
                        default: record("UNKNOWN " + request.Action); break;
                        }
                        record("ACK " + request.Action);
                    }
                    int epoch = generation;
                    var state = new PlaybackState { position = command(new object[] { "get_property", "time-pos" }, true),
                        pause = command(new object[] { "get_property", "pause" }, true),
                        subtitle = command(new object[] { "get_property", "sub-text" }, true),
                        idle = command(new object[] { "get_property", "idle-active" }, true),
                        decoder = command(new object[] { "get_property", "hwdec-current" }, true), generation = epoch };
                    Publish(state, epoch);
                    if (AdvanceSmoke(state, ref smokeStage, smokeSince, record)) break;
                }
                if (!process.HasExited) command(new object[] { "quit" }, false);
            } catch (Exception error) { record("WORKER ERROR " + error); Environment.ExitCode = 1; }
            finally {
                if (pipe != null) pipe.Dispose();
                if (process != null) {
                    if (!process.WaitForExit(5000)) { record("FORCED CLEANUP"); process.Kill(); process.WaitForExit(); Environment.ExitCode = 1; }
                    record("PLAYER EXIT " + process.ExitCode); process.Dispose();
                }
                string note;
                while (notes.TryDequeue(out note)) record(note);
                record("WORKER STOPPED");
                if (smoke && !smokeDone) { record("SMOKE INCOMPLETE"); Environment.ExitCode = 1; }
                if (smoke && !stopping) BeginInvoke((Action)delegate { Close(); });
            }
        }
    }

    [STAThread]
    static void Main(string[] args)
    {
        if (args.Length < 2) { Environment.ExitCode = 2; return; }
        bool smoke = false, direct = false;
        for (int i = 2; i < args.Length; i++) {
            if (args[i] == "--smoke" && !smoke) smoke = true;
            else if (args[i] == "--direct-webview" && !direct) direct = true;
            else { Environment.ExitCode = 2; return; }
        }
        // Read the small generated HTML before starting an interactive UI loop.
        string page = File.ReadAllText(Path.Combine(args[0], "composition.html"));
        // The CLR's architecture determines which native DLL it can load.
        hostArchitecture = RuntimeInformation.ProcessArchitecture.ToString().ToLowerInvariant();
        CoreWebView2Environment.SetLoaderDllFolderPath(Path.Combine(args[0], hostArchitecture));
        SetProcessDpiAwarenessContext(new IntPtr(-4));
        Application.EnableVisualStyles(); Application.SetCompatibleTextRenderingDefault(false);
        Application.Run(new Probe(args[0], args[1], page, smoke, direct));
    }
}
