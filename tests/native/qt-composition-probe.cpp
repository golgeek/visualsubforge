// Isolated Qt/libmpv composition comparison, not an editor or accepted adapter.
// Control worker owns all ordinary mpv calls. Qt main owns the GL/render API.
// Shutdown removes the renderer before destroying the core; main never joins
// a live worker. The fixed fixture has one session, not a document-edit protocol.
#include <QApplication>
#include <QCloseEvent>
#include <QDialog>
#include <QElapsedTimer>
#include <QLabel>
#include <QOpenGLContext>
#include <QOpenGLFunctions>
#include <QOpenGLWidget>
#include <QPushButton>
#include <QSurfaceFormat>
#include <QTimer>
#include <QVBoxLayout>
#include <mpv/client.h>
#include <mpv/render_gl.h>
#include <atomic>
#include <chrono>
#include <clocale>
#include <cmath>
#include <cstdio>
#include <deque>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

using Command = std::vector<std::string>;
struct Shared {
    mpv_handle *core = nullptr; // Published by ready; retained until renderGone.
    std::atomic<bool> ready{false}, renderReady{false}, pending{false};
    std::atomic<bool> stopping{false}, renderGone{false}, done{false}, failed{false};
    std::atomic<unsigned> renders{0};
    std::mutex mutex;
    std::deque<Command> commands;
    double position = 0;
    bool loaded = false, paused = true;
    std::string subtitle;
    void request(Command command) {
        std::lock_guard lock(mutex);
        if (!stopping && commands.size() < 32) commands.push_back(std::move(command));
    }
    bool check(int result, const char *operation) {
        if (result >= 0) return true;
        std::fprintf(stderr, "FAIL %s: %s\n", operation, mpv_error_string(result));
        failed = true; return false;
    }
};
static void control(Shared &s, const std::string &path) {
    s.core = mpv_create();
    if (!s.core) { s.failed = true; s.done = true; return; }
    for (const auto &[name, value] : std::vector<std::pair<const char *, const char *>>{
             {"config","no"},{"vo","libmpv"},{"hwdec","auto-copy"},
             {"idle","yes"},{"keep-open","yes"},{"pause","yes"},
             {"mute","yes"},{"clipboard-backends",""},{"vd-lavc-threads","4"}})
        s.check(mpv_set_option_string(s.core,name,value),name);
    if (!s.check(mpv_initialize(s.core),"initialize") || s.failed) {
        mpv_terminate_destroy(s.core); s.done = true; return;
    }
    mpv_request_log_messages(s.core,"warn");
    s.ready = true;
    bool opened = false;
    while (!s.stopping) {
        if (s.renderReady && !opened) { s.request({"loadfile",path}); opened = true; }
        std::deque<Command> commands;
        { std::lock_guard lock(s.mutex); commands.swap(s.commands); }
        for (const auto &command : commands) {
            std::vector<const char *> args;
            for (const auto &part : command) args.push_back(part.c_str());
            args.push_back(nullptr);
            s.check(mpv_command(s.core,args.data()),args[0]);
        }
        for (int i=0;i<64;i++) {
            mpv_event *event=mpv_wait_event(s.core,0);
            if (event->event_id == MPV_EVENT_NONE) break;
            if (event->event_id == MPV_EVENT_FILE_LOADED) {
                { std::lock_guard lock(s.mutex); s.loaded=true; }
                s.request({"sub-add","memory://1\n00:00:00,000 --> 00:00:06,000\nQt composition: café — 日本語\nSecond line\n","select"});
            }
            if (event->event_id == MPV_EVENT_LOG_MESSAGE) {
                auto *log=static_cast<mpv_event_log_message *>(event->data);
                std::fprintf(stderr,"MPV [%s] %s",log->prefix,log->text);
            }
            if (event->event_id == MPV_EVENT_END_FILE &&
                static_cast<mpv_event_end_file *>(event->data)->reason == MPV_END_FILE_REASON_ERROR)
                s.failed = true;
        }
        double pos=0; int paused=1;
        mpv_get_property(s.core,"time-pos",MPV_FORMAT_DOUBLE,&pos);
        mpv_get_property(s.core,"pause",MPV_FORMAT_FLAG,&paused);
        char *sub=mpv_get_property_string(s.core,"sub-text");
        { std::lock_guard lock(s.mutex); s.position=pos; s.paused=paused; s.subtitle=sub?sub:""; }
        mpv_free(sub);
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }
    while (!s.renderGone) std::this_thread::sleep_for(std::chrono::milliseconds(1));
    mpv_terminate_destroy(s.core);
    std::puts("CORE DESTROYED"); s.done=true;
}
class Video final : public QOpenGLWidget {
    Shared &s;
    mpv_render_context *renderer=nullptr;
    static void *proc(void *, const char *name) {
        return reinterpret_cast<void *>(QOpenGLContext::currentContext()->getProcAddress(name));
    }
    static void dirty(void *raw) { static_cast<Shared *>(raw)->pending=true; }
public:
    explicit Video(Shared &state) : s(state) { setMinimumSize(100,100); }
    void release() {
        if (renderer) {
            makeCurrent(); mpv_render_context_set_update_callback(renderer,nullptr,nullptr);
            mpv_render_context_free(renderer); renderer=nullptr; doneCurrent();
            std::puts("RENDERER REMOVED");
        }
        s.renderGone=true;
    }
    void service() {
        if (!s.ready || s.stopping) return;
        if (!renderer || s.pending.exchange(false)) {
            if (isVisible() && !window()->isMinimized()) update();
            else if (renderer) {
                makeCurrent(); mpv_render_context_update(renderer);
                int skip=1; mpv_render_param p[]={{MPV_RENDER_PARAM_SKIP_RENDERING,&skip},{MPV_RENDER_PARAM_INVALID,nullptr}};
                s.check(mpv_render_context_render(renderer,p),"hidden render"); doneCurrent();
            }
        }
    }
protected:
    void initializeGL() override {
        std::printf("GL %s / %s DPR=%.2f\n",context()->functions()->glGetString(GL_VERSION),
                    context()->functions()->glGetString(GL_RENDERER),devicePixelRatioF());
    }
    void paintGL() override {
        if (!s.ready || s.stopping) return;
        if (!renderer) {
            mpv_opengl_init_params gl{proc,nullptr}; int advanced=1;
            mpv_render_param p[]={{MPV_RENDER_PARAM_API_TYPE,const_cast<char *>(MPV_RENDER_API_TYPE_OPENGL)},
                {MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,&gl},{MPV_RENDER_PARAM_ADVANCED_CONTROL,&advanced},
                {MPV_RENDER_PARAM_INVALID,nullptr}};
            if (!s.check(mpv_render_context_create(&renderer,s.core,p),"render create")) return;
            mpv_render_context_set_update_callback(renderer,dirty,&s); s.renderReady=true;
        }
        mpv_render_context_update(renderer);
        mpv_opengl_fbo fbo{static_cast<int>(defaultFramebufferObject()),
            qRound(width()*devicePixelRatioF()),qRound(height()*devicePixelRatioF()),0};
        int flip=1,block=0;
        mpv_render_param p[]={{MPV_RENDER_PARAM_OPENGL_FBO,&fbo},{MPV_RENDER_PARAM_FLIP_Y,&flip},
            {MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME,&block},{MPV_RENDER_PARAM_INVALID,nullptr}};
        if (s.check(mpv_render_context_render(renderer,p),"render")) ++s.renders;
    }
};
class Window final : public QWidget {
public:
    Shared &s;
    explicit Window(Shared &state) : s(state) {}
    void closeEvent(QCloseEvent *event) override {
        if (s.done && s.renderGone) event->accept();
        else { event->ignore(); s.stopping=true; }
    }
};
int main(int argc,char **argv) {
    if (argc!=2) { std::fprintf(stderr,"usage: %s media\n",argv[0]); return 2; }
    const std::string media=argv[1];
    setvbuf(stdout,nullptr,_IOLBF,0);
    QSurfaceFormat format; format.setVersion(3,2); format.setProfile(QSurfaceFormat::CoreProfile);
    QSurfaceFormat::setDefaultFormat(format);
    QApplication app(argc,argv); std::setlocale(LC_NUMERIC,"C");
    Shared s; Window window(s);
    window.setWindowTitle("VSF Qt composition comparison"); window.resize(1100,720); window.setMinimumSize(800,600);
    auto *layout=new QVBoxLayout(&window);
    auto *video=new Video(s); layout->addWidget(video,1);
    auto *status=new QLabel("Loading fixed fixture"); layout->addWidget(status);
    auto *play=new QPushButton("Play / pause"); layout->addWidget(play);
    QObject::connect(play,&QPushButton::clicked,[&]{s.request({"cycle","pause"});});
    auto *dialog=new QPushButton("Open modal"); layout->addWidget(dialog);
    QObject::connect(dialog,&QPushButton::clicked,[&]{
        auto *modal=new QDialog(&window); modal->setAttribute(Qt::WA_DeleteOnClose);
        auto *content=new QVBoxLayout(modal); content->addWidget(new QLabel("Native modal over GPU video"));
        auto *close=new QPushButton("Close"); content->addWidget(close);
        QObject::connect(close,&QPushButton::clicked,modal,&QDialog::accept); modal->setModal(true); modal->show();
    });
    std::printf("QT %s platform=%s\n",qVersion(),qPrintable(QGuiApplication::platformName()));
    std::thread worker(control,std::ref(s),media);
    QElapsedTimer elapsed; elapsed.start(); qint64 last=0,maxGap=0;
    int stage=0; qint64 since=0; bool passed=false;
    QTimer timer;
    QObject::connect(&timer,&QTimer::timeout,[&]{
        const qint64 now=elapsed.elapsed(); if(last) maxGap=qMax(maxGap,now-last); last=now;
        if (s.stopping || s.failed || now>20000) {
            s.stopping=true; if (!s.renderGone) video->release();
            if (s.done) app.quit();
            return;
        }
        video->service();
        double pos; bool paused,loaded; std::string sub;
        { std::lock_guard lock(s.mutex); pos=s.position; paused=s.paused; loaded=s.loaded; sub=s.subtitle; }
        status->setText(QString("%1 s / renders %2 / stage %3").arg(pos,0,'f',3).arg(s.renders.load()).arg(stage));
        if (!loaded || !s.renders || sub.empty() || now-since<1000) return;
        if (stage==0) { s.request({"seek","2","absolute+exact"}); stage++; since=now; }
        else if (stage==1 && paused && std::abs(pos-2)<.05) { s.request({"set","pause","no"}); stage++; since=now; }
        else if (stage==2 && !paused && pos>2.5) { s.request({"set","pause","yes"}); stage++; since=now; }
        else if (stage==3 && paused) {
            std::printf("SMOKE PASS position=%.3f renders=%u max_tick_gap_ms=%lld\n",pos,s.renders.load(),static_cast<long long>(maxGap));
            passed=true; s.stopping=true;
        }
    });
    timer.start(8); window.show(); const int result=app.exec();
    worker.join(); // UI loop exited only after done; never waits for live work.
    std::printf("STOPPED passed=%d failed=%d renders=%u\n",passed,s.failed.load(),s.renders.load());
    return result || s.failed || !passed ? 1 : 0;
}
