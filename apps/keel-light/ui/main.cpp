#include "MainWindow.h"

#include <QAction>
#include <QApplication>
#include <QMenuBar>
#include <QStyleFactory>

#if defined(Q_OS_MACOS)
extern "C" {
#include "sparkle_bridge.h"
}
#endif

int main(int argc, char *argv[]) {
  QApplication app(argc, argv);
  app.setApplicationName(QStringLiteral("Keel Light"));
  app.setOrganizationName(QStringLiteral("Keel"));
  app.setStyle(QStyleFactory::create(QStringLiteral("Fusion")));

#if defined(Q_OS_MACOS)
  keel_sparkle_init();
#endif

  quint16 port = 1886;
  if (const auto env = qgetenv("KEEL_LIGHT_PORT"); !env.isEmpty())
    port = static_cast<quint16>(env.toUShort());
  for (int i = 1; i < argc; ++i) {
    const QByteArray a = argv[i];
    if (a == "--port" && i + 1 < argc)
      port = static_cast<quint16>(QByteArray(argv[++i]).toUShort());
  }

  MainWindow w(port);

#if defined(Q_OS_MACOS)
  if (auto *menuBar = w.menuBar()) {
    auto *help = menuBar->addMenu(QStringLiteral("Help"));
    auto *updates = help->addAction(QStringLiteral("Check for Updates…"));
    QObject::connect(updates, &QAction::triggered, []() { keel_sparkle_check_for_updates(); });
  }
#endif

  w.show();
  return app.exec();
}
