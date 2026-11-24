// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QOpenGLContext>
#include <QOpenGLFunctions>
#include <QQuickWindow>
#include <QString>
#include <atomic>
#include <utility>

// Render-thread hooks for the private GPU timing scene. Qt's OpenGL ES RHI
// does not always expose a timestamp even when the driver has timer queries.
class CaptureGpuTimer {
public:
  explicit CaptureGpuTimer(QString label = {}) : m_label(std::move(label)) {}
  void begin(QQuickWindow *window) {
    window->beginExternalCommands();
    auto *context = QOpenGLContext::currentContext();
    if (context && !m_checked) {
      m_checked = true;
      m_ext = context->hasExtension("GL_EXT_disjoint_timer_query");
      const bool core = !context->isOpenGLES() &&
                        (context->format().version() >= qMakePair(3, 3) ||
                         context->hasExtension("GL_ARB_timer_query"));
      if (m_ext || core) {
        const auto proc = [&](const char *name) {
          const QByteArray base(name);
          const auto ext =
              m_ext ? context->getProcAddress(base + "EXT") : nullptr;
          return ext ? ext : context->getProcAddress(base);
        };
        m_gen = reinterpret_cast<Gen>(proc("glGenQueries"));
        m_begin = reinterpret_cast<Begin>(proc("glBeginQuery"));
        m_end = reinterpret_cast<End>(proc("glEndQuery"));
        m_get = reinterpret_cast<Get>(proc("glGetQueryObjectuiv"));
        m_delete = reinterpret_cast<Delete>(proc("glDeleteQueries"));
      }
      if (!m_gen || !m_begin || !m_end || !m_get || !m_delete)
        qWarning("GPU elapsed-time query unavailable on this context");
    }
    if (context && m_gen && m_begin && m_end && m_get && m_delete) {
      m_gen(1, &m_query);
      if (m_query)
        m_begin(timeElapsed, m_query);
    }
    window->endExternalCommands();
  }

  void finish(QQuickWindow *window) {
    if (!m_query)
      return;
    window->beginExternalCommands(); // Submit Qt's preceding scene draws.
    m_end(timeElapsed);
    GLuint ns = 0;
    m_get(m_query, 0x8866 /* GL_QUERY_RESULT */, &ns);
    GLint disjoint = 0;
    if (m_ext)
      QOpenGLContext::currentContext()->functions()->glGetIntegerv(
          0x8FBB /* GL_GPU_DISJOINT_EXT */, &disjoint);
    m_delete(1, &m_query);
    m_query = 0;
    window->endExternalCommands();
    if (ns && !disjoint) {
      ++samples;
      qInfo("GL GPU scene time %.6f ms window %s", ns * 0.000001, qPrintable(m_label));
    }
  }

  std::atomic<int> samples{0};

private:
  QString m_label;
  using Gen = void (*)(GLsizei, GLuint *);
  using Begin = void (*)(GLenum, GLuint);
  using End = void (*)(GLenum);
  using Get = void (*)(GLuint, GLenum, GLuint *);
  using Delete = void (*)(GLsizei, const GLuint *);
  static constexpr GLenum timeElapsed = 0x88BF;
  Gen m_gen = nullptr;
  Begin m_begin = nullptr;
  End m_end = nullptr;
  Get m_get = nullptr;
  Delete m_delete = nullptr;
  bool m_checked = false, m_ext = false;
  GLuint m_query = 0;
};
