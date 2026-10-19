#include "MainWindow.h"
#include "RpcClient.h"

#include <QCheckBox>
#include <QComboBox>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QListWidget>
#include <QMenuBar>
#include <QMessageBox>
#include <QPushButton>
#include <QSlider>
#include <QStatusBar>
#include <QTableWidget>
#include <QTabWidget>
#include <QVBoxLayout>
#include <QApplication>
#include <QStyleFactory>

MainWindow::MainWindow(quint16 port, QWidget *parent)
    : QMainWindow(parent), port_(port), rpc_(new RpcClient(this)) {
  setWindowTitle(QStringLiteral("Keel Audio"));
  resize(1100, 720);
  buildUi();
  connect(rpc_, &RpcClient::connectedChanged, this, &MainWindow::onConnected);
  connect(&refreshTimer_, &QTimer::timeout, this, &MainWindow::refreshAll);
  rpc_->connectTo(port_);
  refreshTimer_.start(1000);
}

void MainWindow::buildUi() {
  auto *central = new QWidget(this);
  setCentralWidget(central);
  auto *root = new QVBoxLayout(central);

  auto *top = new QHBoxLayout;
  top->addWidget(new QLabel(QStringLiteral("Keel Audio"), this));
  top->addStretch();
  top->addWidget(new QLabel(QStringLiteral("Main output"), this));
  mainOut_ = new QComboBox(this);
  mainOut_->setMinimumWidth(220);
  top->addWidget(mainOut_);
  auto *applyOut = new QPushButton(QStringLiteral("Set"), this);
  connect(applyOut, &QPushButton::clicked, this, [this] {
    const auto id = mainOut_->currentData().toString();
    if (id.isEmpty())
      return;
    rpc_->call("setOutputDevice",
               QJsonObject{{"mainOutput",
                            QJsonObject{{"outputDeviceId", id}, {"outputId", "generic"}}}},
               [this](QJsonValue) { refreshAll(); });
  });
  top->addWidget(applyOut);
  root->addLayout(top);

  tabs_ = new QTabWidget(this);
  root->addWidget(tabs_, 1);

  // Mix matrix tab
  auto *mixPage = new QWidget;
  auto *mixLay = new QHBoxLayout(mixPage);
  channelList_ = new QListWidget;
  mixList_ = new QListWidget;
  matrix_ = new QTableWidget;
  matrix_->horizontalHeader()->setSectionResizeMode(QHeaderView::Stretch);
  matrix_->verticalHeader()->setSectionResizeMode(QHeaderView::ResizeToContents);
  auto *leftCol = new QVBoxLayout;
  leftCol->addWidget(new QLabel(QStringLiteral("Channels")));
  leftCol->addWidget(channelList_);
  auto *addCh = new QPushButton(QStringLiteral("Add channel"));
  connect(addCh, &QPushButton::clicked, this, [this] {
    // Channels are created via engine defaults; refresh surfaces new state after RPC add via getMixerState mutate locally through setRouting after user picks.
    refreshAll();
  });
  leftCol->addWidget(addCh);
  mixLay->addLayout(leftCol, 1);

  auto *mid = new QVBoxLayout;
  mid->addWidget(new QLabel(QStringLiteral("Channel → Mix routing")));
  mid->addWidget(matrix_, 1);
  mixLay->addLayout(mid, 3);

  auto *rightCol = new QVBoxLayout;
  rightCol->addWidget(new QLabel(QStringLiteral("Mixes")));
  rightCol->addWidget(mixList_);
  issuesList_ = new QListWidget;
  rightCol->addWidget(new QLabel(QStringLiteral("Issues")));
  rightCol->addWidget(issuesList_);
  mixLay->addLayout(rightCol, 1);
  tabs_->addTab(mixPage, QStringLiteral("Mixer"));

  // Effects
  auto *fxPage = new QWidget;
  auto *fxLay = new QVBoxLayout(fxPage);
  effectList_ = new QListWidget;
  fxLay->addWidget(new QLabel(QStringLiteral("Built-in + AU effects")));
  fxLay->addWidget(effectList_);
  auto *addFx = new QPushButton(QStringLiteral("Add EQ to first channel"));
  connect(addFx, &QPushButton::clicked, this, [this] {
    const auto channels = lastState_.value("channels").toArray();
    if (channels.isEmpty())
      return;
    const auto id = channels.at(0).toObject().value("id").toString();
    rpc_->call("addEffect", QJsonObject{{"channelId", id}, {"kind", "equalizer"}},
               [this](QJsonValue) { refreshAll(); });
  });
  fxLay->addWidget(addFx);
  tabs_->addTab(fxPage, QStringLiteral("Effects"));

  // Wave hardware
  auto *wavePage = new QWidget;
  auto *waveLay = new QVBoxLayout(wavePage);
  waveList_ = new QListWidget;
  waveLay->addWidget(new QLabel(QStringLiteral("Wave USB devices")));
  waveLay->addWidget(waveList_);
  auto *refreshWave = new QPushButton(QStringLiteral("Refresh devices"));
  connect(refreshWave, &QPushButton::clicked, this, [this] {
    rpc_->call("getDevices", {}, [this](QJsonValue v) { rebuildDevices(v); });
  });
  waveLay->addWidget(refreshWave);
  tabs_->addTab(wavePage, QStringLiteral("Wave devices"));

  // Settings
  auto *setPage = new QWidget;
  auto *setLay = new QVBoxLayout(setPage);
  setLay->addWidget(new QLabel(QStringLiteral("Offline · no accounts · no telemetry")));
  setLay->addWidget(new QLabel(QStringLiteral("URL scheme: keelaudio://")));
  auto *halLabel = new QLabel(QStringLiteral("HAL driver: checking…"));
  setLay->addWidget(halLabel);
  auto *halBtn = new QPushButton(QStringLiteral("Install / update virtual driver…"));
  connect(halBtn, &QPushButton::clicked, this, [this, halLabel] {
    rpc_->call("installHalDriver", QJsonObject{{"force", true}},
               [halLabel](QJsonValue v) {
                 const auto o = v.toObject();
                 const auto r = o.value("report").toObject();
                 halLabel->setText(
                     QStringLiteral("HAL: %1 (build %2 / bundled %3)")
                         .arg(r.value("status").toString())
                         .arg(r.value("installed_build").toInt())
                         .arg(r.value("bundled_build").toInt()));
               },
               [halLabel](QString err) {
                 halLabel->setText(QStringLiteral("HAL install failed: %1").arg(err));
               });
  });
  setLay->addWidget(halBtn);
  setLay->addWidget(new QLabel(
      QStringLiteral("On launch, keel-audio prompts for admin when the driver is missing or outdated.")));
  auto *themeSys = new QPushButton(QStringLiteral("Theme: System"));
  connect(themeSys, &QPushButton::clicked, this, &MainWindow::applyTheme);
  setLay->addWidget(themeSys);
  setLay->addStretch();
  tabs_->addTab(setPage, QStringLiteral("Settings"));
  rpc_->call("getHalStatus", {}, [halLabel](QJsonValue v) {
    const auto r = v.toObject();
    halLabel->setText(QStringLiteral("HAL: %1 (build %2 / bundled %3)")
                          .arg(r.value("status").toString())
                          .arg(r.value("installed_build").toInt())
                          .arg(r.value("bundled_build").toInt()));
  });

  status_ = new QLabel(QStringLiteral("Connecting…"));
  statusBar()->addWidget(status_);

  auto *file = menuBar()->addMenu(QStringLiteral("File"));
  file->addAction(QStringLiteral("Refresh"), this, &MainWindow::refreshAll);
  file->addAction(QStringLiteral("Quit"), qApp, &QApplication::quit);
}

void MainWindow::onConnected(bool on) {
  status_->setText(on ? QStringLiteral("Connected to engine")
                      : QStringLiteral("Disconnected — is keel-audio running?"));
  if (on)
    refreshAll();
}

void MainWindow::refreshAll() {
  rpc_->call("getMixerState", {}, [this](QJsonValue v) {
    lastState_ = v.toObject();
    rebuildMatrix(lastState_);
  });
  rpc_->call("getOutputDevices", {}, [this](QJsonValue v) {
    const auto obj = v.toObject();
    mainOut_->clear();
    const auto devices = obj.value("outputDevices").toArray();
    for (const auto &d : devices) {
      const auto o = d.toObject();
      mainOut_->addItem(o.value("name").toString(), o.value("id").toString());
    }
    issuesList_->clear();
    for (const auto &i : obj.value("issues").toArray())
      issuesList_->addItem(i.toString());
    const bool needs = obj.value("halNeedsUpdate").toBool();
    const bool installed = obj.value("halInstalled").toBool();
    if (needs)
      status_->setText(QStringLiteral("Engine · HAL needs install/update (Settings)"));
    else if (installed)
      status_->setText(QStringLiteral("Engine · HAL installed (build %1)")
                           .arg(obj.value("halInstalledBuild").toInt()));
    else
      status_->setText(QStringLiteral("Engine · HAL not installed (see Settings)"));
  });
  rpc_->call("listEffects", {}, [this](QJsonValue v) { rebuildEffects(v); });
  rpc_->call("getDevices", {}, [this](QJsonValue v) { rebuildDevices(v); });
}

void MainWindow::rebuildMatrix(const QJsonObject &state) {
  channelList_->clear();
  mixList_->clear();
  const auto channels = state.value("channels").toArray();
  const auto mixes = state.value("mixes").toArray();
  for (const auto &c : channels) {
    const auto o = c.toObject();
    const auto name = o.value("name").toString();
    const bool mute = o.value("mute").toBool();
    channelList_->addItem(QStringLiteral("%1%2")
                              .arg(name, mute ? QStringLiteral(" [MUTE]") : QString()));
  }
  for (const auto &m : mixes) {
    const auto o = m.toObject();
    mixList_->addItem(o.value("name").toString());
  }

  matrix_->clear();
  matrix_->setRowCount(channels.size());
  matrix_->setColumnCount(mixes.size());
  QStringList h, v;
  for (const auto &m : mixes)
    h << m.toObject().value("name").toString();
  for (const auto &c : channels)
    v << c.toObject().value("name").toString();
  matrix_->setHorizontalHeaderLabels(h);
  matrix_->setVerticalHeaderLabels(v);

  for (int r = 0; r < channels.size(); ++r) {
    const auto ch = channels.at(r).toObject();
    const auto chId = ch.value("id").toString();
    for (int c = 0; c < mixes.size(); ++c) {
      const auto mix = mixes.at(c).toObject();
      const auto mixId = mix.value("id").toString();
      const auto routing = mix.value("routing").toObject();
      const bool on = routing.contains(chId);
      auto *cell = new QWidget;
      auto *lay = new QHBoxLayout(cell);
      lay->setContentsMargins(4, 2, 4, 2);
      auto *cb = new QCheckBox;
      cb->setChecked(on);
      auto *sl = new QSlider(Qt::Horizontal);
      sl->setRange(0, 100);
      sl->setValue(int(routing.value(chId).toDouble(1.0) * 100));
      sl->setEnabled(on);
      lay->addWidget(cb);
      lay->addWidget(sl);
      matrix_->setCellWidget(r, c, cell);

      connect(cb, &QCheckBox::toggled, this, [this, mixId, chId, sl](bool checked) {
        sl->setEnabled(checked);
        QJsonObject params{{"mixId", mixId}, {"channelId", chId}};
        if (checked)
          params.insert("level", sl->value() / 100.0);
        else
          params.insert("level", QJsonValue::Null);
        // Null level removes route — send without level key when off
        if (!checked) {
          rpc_->call("setRouting", QJsonObject{{"mixId", mixId}, {"channelId", chId}},
                     [](QJsonValue) {});
        } else {
          rpc_->call("setRouting",
                     QJsonObject{{"mixId", mixId},
                                 {"channelId", chId},
                                 {"level", sl->value() / 100.0}},
                     [](QJsonValue) {});
        }
      });
      connect(sl, &QSlider::valueChanged, this, [this, mixId, chId](int v) {
        rpc_->call("setRouting",
                   QJsonObject{{"mixId", mixId}, {"channelId", chId}, {"level", v / 100.0}},
                   [](QJsonValue) {});
      });
    }
  }
}

void MainWindow::rebuildDevices(const QJsonValue &devices) {
  waveList_->clear();
  const auto arr = devices.toObject().value("devices").toArray();
  for (const auto &d : arr) {
    const auto o = d.toObject();
    const auto info = o.value("info").toObject();
    const auto transport = info.value("transport").toString();
    waveList_->addItem(
        QStringLiteral("%1 — %2 — gain %3 dB%4")
            .arg(info.value("name").toString())
            .arg(transport.isEmpty() ? QStringLiteral("usb") : transport)
            .arg(o.value("input_gain_db").toDouble())
            .arg(o.value("mute").toBool() ? QStringLiteral(" [MUTE]") : QString()));
  }
  if (waveList_->count() == 0)
    waveList_->addItem(
        QStringLiteral("No Wave devices found (Core Audio / HID). Is the mic plugged in?"));
}

void MainWindow::rebuildEffects(const QJsonValue &effects) {
  effectList_->clear();
  const auto obj = effects.toObject();
  for (const auto &b : obj.value("builtin").toArray()) {
    const auto o = b.toObject();
    effectList_->addItem(QStringLiteral("Built-in: %1").arg(o.value("title").toString()));
  }
  for (const auto &a : obj.value("au").toArray())
    effectList_->addItem(QStringLiteral("AU: %1").arg(a.toString()));
}

void MainWindow::applyTheme() {
  qApp->setStyle(QStyleFactory::create(QStringLiteral("Fusion")));
}
