# Linux systemd 개발 backend와 관리자 등록

`linux-service register`로 개발용 control을 직접 만드는 관리자 인터페이스와 권한 모델을 설명한다. 일반적인 신규 설치는 계정 준비·승인·이름 연결을 제공하는 [Linux 설치·운용 안내](./linux-operations.md)의 `setup new/apply`를 사용한다. 아래 저수준 등록은 그 흐름을 대체하는 고객 설치 절차가 아니다. 정식 signed runtime/exact executor profile과 실제 Linux 노드·systemd 인수는 남아 있다.

## 현재 관리 프로토콜과 기존 등록의 호환

현재 버전으로 새 `linux-service register`를 완료하면 root 공개 `management.json`을 발행한다. 현재 관리 CLI는 이 descriptor의 control identity·protocol·최초 worker pin을 대조하고 등록된 worker에만 정해진 운용 요청을 위임한다. 사용자 순차 setup은 별도 고정 설치 helper와 사용자 catalog를 사용하며 아래 수동 spec/사전 계정 절차와 구분한다.

LNX-03에서 이미 만든 control에 descriptor가 없으면 현재 관리 CLI는 `LINUX_EXECUTOR_DESCRIPTOR_REQUIRED`로 거부한다. 이전 worker를 새 프로토콜 실행기로 인증하거나 descriptor를 임의 보충하지 않는다. 이전 개발 기록은 당시의 원래 LNX-03 실행기로 다루고 자료를 보존한다. 기존 등록 migration은 후속이며 새 UX를 검증할 때는 새 시험 이름·새 control을 사용한다.

Ubuntu 24.04.4 실기의 지정 RC.4 개발 후보에서 시스템 설치·초기화·SSH 단절 후 서비스 유지·재기동·정상 종료를 확인했다. 서로 다른 계정 간 권한 격리와 장애 경로는 별도 검증 대상이다. CLI CI와 Mac 실행 결과를 Linux 엔진·서비스 인수로 계산하지 않는다. SSH 접속 정보를 개발자에게 제공할 필요는 없다. 관리자가 아래 준비와 별도 실물 시험을 수행하고 정제한 결과를 반환할 수 있다.

## 대상과 권한

- Ubuntu 24.04 LTS, x86_64/glibc, PID 1 systemd 255 이상. 등록·초기화·시작 gate는 system manager의 `running` 상태도 요구한다. WSL2는 후속이다.
- 서비스 backend는 고정 `/usr/bin/systemctl`, `/usr/bin/setpriv`를 사용한다. 임의 shell/PATH 탐색은 하지 않는다. 일반 사용자 관리 CLI는 고정 worker에 `sudo -n`으로 위임하므로 먼저 `sudo -v`로 인증한다.
- root가 관리하는 CLI executable과 persistent instance parent가 필요하다. 사용자 home·공용 `/tmp` 아래를 system service 제어 경로로 사용하지 않는다.
- 이 저수준 등록에서는 로컬 `/etc/passwd`·`/etc/group`에 전용 nologin 계정과 비어 있는 전용 그룹을 인스턴스마다 관리자가 사전 생성한다. 일반 `setup apply`는 승인 후 이를 준비한다. LDAP/SSSD 등 NSS 전체의 계정 검증을 제공하지 않는다. 일반 로그인 계정·root UID/GID·다른 인스턴스 계정을 재사용하지 않는다.
- 등록은 root 실행을 요구한다. 등록 뒤 일반 사용자 관리 CLI의 init/start/status/stop도 인증된 고정 worker에서 권한 작업을 수행한다. 사용자별 `~/.local/share/BXDL` catalog에 root 기록을 섞지 않으며 sudoers나 정책 파일을 자동 수정하지 않는다.

## 등록 준비

먼저 `bxdl host check --json`으로 읽기 전용 기준 검사를 한다. manager가 `degraded`이면 등록·init·새 시작은 거절한다. 이미 실행한 시도의 `status/stop`은 이 제한을 다시 적용하지 않아 종료 관측과 정리를 시도할 수 있다. host 성공은 엔진·native·계정 격리 인수 성공을 뜻하지 않는다.

[후보 제작·검증 절차](./linux-native-build.md)에서 고정한 공급물과 [Linux cold 검사](./linux-validation.md)를 사용한다. 공급물 기준은 전달본의 plan/receipt/lock이며 과거 source snapshot을 최신 실행기로 간주하지 않는다. Java/JAR의 cold 결과만으로 native/RocksDB 검증을 완료했다고 판단하지 않는다. CLI는 native Linux에서 빌드한다. Mac에서 생성한 CLI는 사용하지 않는다.

관리자는 [고정 helper 최초 배치](./linux-operations.md#2-관리자-최초-준비)에 따라 검증한 Linux 실행 파일을 `/usr/local/libexec/bxdl/bxdl`에 배치한다. 이 절차는 기존 파일·디렉터리의 소유자나 권한을 덮어쓰지 않는다. 아래 control의 부모도 root 소유·비symlink·group/other 쓰기 금지 조건으로 미리 준비해야 한다. 기존 control은 사용하지 않는다.

```sh
/usr/local/libexec/bxdl/bxdl linux-service account --instance /var/lib/bxdl/instances/linux-dev-1 --json
```

마지막 명령은 계정 이름을 계산할 뿐 계정·파일·서비스를 만들지 않는다. 반환된 `data.account`를 다음 명령의 `<계정>`에 사용한다. `--instance` 경로는 등록까지 동일하게 유지한다. 현재 unit 경로는 공백·한글·systemd 확장 문자가 없는 정규 절대 경로를 사용한다.

```sh
SERVICE_ACCOUNT='<앞 명령의 data.account 값>'
sudo groupadd --system "$SERVICE_ACCOUNT"
sudo useradd --system --gid "$SERVICE_ACCOUNT" --home-dir /nonexistent --no-create-home --shell /usr/sbin/nologin "$SERVICE_ACCOUNT"
id "$SERVICE_ACCOUNT"
```

다음 형식의 spec을 준비한다. UID/GID는 예시 숫자를 그대로 쓰지 않고 실제 전용 계정 값을 입력한다. 모든 경로는 절대 경로다. spec 파일과 그 상위 경로는 root 소유이며 group/other가 쓰지 못해야 한다. 제품/native 설정은 동일한 체인·키·포트·새 데이터 경로를 가리켜야 하며, native 설정의 `dataDirectory`는 새 instance의 `data` 경로여야 한다. 기존 운영 데이터는 가져오지 않는다.

```json
{
  "schemaVersion": 1,
  "runtimeRoot": "/srv/bxdl-input/runtime",
  "runtimeLock": "/srv/bxdl-input/runtime-lock.temurin-21-linux-amd64.json",
  "engineJar": "/srv/bxdl-input/nigo-node-0.0.1-SNAPSHOT.jar",
  "engineLock": "/srv/bxdl-input/engine-lock.rc4-temurin-linux-development.json",
  "nativeConfig": "/srv/bxdl-input/node.json",
  "productConfig": "/srv/bxdl-input/instance.json",
  "serviceUid": 991,
  "serviceGid": 991
}
```

선정 runtime/engine lock의 bytes, RC.4 JAR/Java hash와 전체 JRE inventory를 대조한다. 입력은 root 제어 영역에 보관한 뒤 사용하고 Java는 root로 실행하지 않는다. 원래 입력 경로를 계속 실행 권한의 근거로 쓰지 않는다. 등록 중 실패한 새 폴더는 자동 삭제하거나 기존 정상 등록으로 채택하지 않는다.

```sh
sudo /usr/local/libexec/bxdl/bxdl linux-service register \
  --instance /var/lib/bxdl/instances/linux-dev-1 \
  --spec /srv/bxdl-input/linux-service.json --allow-development --json
```

등록은 명시 초기화·서비스 시작과 별도다. `--allow-development`는 정식 공급·Linux 인수가 끝났다는 승인이 아니다. 이 경로에서 보관한 개발용 worker를 signed Linux package의 승인된 exact executor로 간주하지 않는다. 등록 당시 worker를 이후 시도의 gate로 복사하며 관리 CLI 파일 교체가 기존 worker를 바꾸지 않는다. 현재 관리 CLI는 공개 descriptor를 검사한 뒤 init·운용·진단 요청을 최초 worker에 위임한다. 이 개발용 프로토콜이 정식 Linux exact profile이나 실제 CLI 전환 인수 완료를 뜻하지는 않는다.

## 초기화·운용

아래는 명시 control 경로를 사용하는 관리자 실행 예시다. 일반 사용자 CLI도 `sudo -v` 후 같은 `--instance` 명령을 사용해 고정 worker로 위임할 수 있다. 명령은 하나씩 결과를 확인한 뒤 진행하며 초기화·종료가 실패하거나 UNKNOWN이면 다음 start를 실행하지 않는다.

```sh
sudo /usr/local/libexec/bxdl/bxdl init --instance /var/lib/bxdl/instances/linux-dev-1 --confirm-initialize --json
sudo /usr/local/libexec/bxdl/bxdl start --instance /var/lib/bxdl/instances/linux-dev-1 --json
sudo /usr/local/libexec/bxdl/bxdl status --instance /var/lib/bxdl/instances/linux-dev-1 --wait --json
sudo /usr/local/libexec/bxdl/bxdl stop --instance /var/lib/bxdl/instances/linux-dev-1 --json
```

`resume-init --confirm-resume`은 같은 identity의 미완료 초기화 기록과 잠금 조건을 충족할 때만 사용한다. 초기화 불명 기록을 지우거나 DB를 비운 뒤 성공한 것으로 바꾸지 않는다. 시작 대기 시간 만료는 JVM 종료를 뜻하지 않으며 상태를 다시 관측한다. 자동 재시작과 부팅 enable은 제공하지 않는다.

시도별 systemd unit은 root 소유 worker의 `systemd-run` gate를 호출한다. 직접 `systemctl start`해도 같은 gate를 거치고 소비된 승인은 재사용할 수 없다. 운영 시작은 새 시도를 만드는 `bxdl start`를 사용한다. `systemd-run`과 `systemd-exit`은 내부 명령이므로 운영자가 직접 호출할 필요가 없다. `ExecStopPost`에서 root가 InvocationID/ControlPID/cgroup을 확인하고 OS 종료 기록을 남겨, systemd가 종료 unit의 메모리 정보를 정리한 뒤에도 결속을 확인한다. 종료 기록만으로 엔진 정상 종료를 인정하지 않는다. unit의 drop-in/ExecStart 변경을 자동 수용하지 않는다.

root gate는 unit/InvocationID/PID/cgroup과 root 기록을 결속한다. 고정 `setpriv`가 보조 그룹·capability를 제거하고 전용 UID/GID로 Java를 exec하여 PID를 유지한다. 설정·공급물·journal은 서비스 UID가 수정할 수 없으며 data와 지정 output만 서비스 UID가 소유한다. init/cold에도 같은 권한 전환을 적용한다. root 제어 기록의 쓰기 descriptor는 엔진에 전달하지 않는다.

`active/running`은 로컬 READY가 아니다. 상태는 엔진 report·nodeInstanceId·HTTP bootstrap/health와 process identity를 결합한다. 정상 종료는 해당 엔진의 STOPPED/storage·consensus closure, process/cgroup 부재와 DB lock 해제 조건이 모두 필요하다. 종료 요청 수락만으로 성공을 기록하지 않으며 근거가 모자라면 UNKNOWN으로 보존한다. 이 모든 관측은 네트워크 정족수·거래 확정과 별개다.

## 제한된 저장 기록·진단

현재 descriptor가 있는 등록은 일반 사용자 터미널에서 다음처럼 수집한다. 진단 출력은 시스템 제어 영역 밖의 사용자 소유 디렉터리에 새로 만든다.

```sh
sudo -v
/usr/local/libexec/bxdl/bxdl logs --instance /var/lib/bxdl/instances/linux-dev-1 --json
/usr/local/libexec/bxdl/bxdl diagnose --instance /var/lib/bxdl/instances/linux-dev-1 --output ./linux-dev-1-diagnostic.json --json
```

현재 고정 worker가 최신 init/run 사건과 저장 상태를 정제해 반환한다. HTTP·systemctl·Java를 새로 실행하지 않으며 실시간 상태는 미관측이다. 누락·한도·불완전 사건은 `partial=true`, `INCOMPLETE`(exit 5)로 표시한다. raw stdout/stderr·system journal·과거 모든 시도의 전체 이력을 내보내지는 않는다.

`diagnose`는 일반 사용자 프로세스가 제어/data 영역 밖의 기존에 없는 파일을 사용자 `0600`으로 저장한다. 전체 명령을 sudo로 실행하지 않는다. 기존 파일을 덮거나 symlink를 따라가지 않는다. 자세한 권한·명령은 [Linux 운용 안내](./linux-operations.md)를 따른다.

## 실물 인수에서 확인할 항목

1. 실제 OS/kernel/glibc/systemd, CLI source/hash, runtime/JAR/Java hash를 기록하고 cold 식별 검사와 별도 native/RocksDB 실행을 수행한다.
2. root JVM이 없는지, 엔진 UID/GID·보조 그룹·capability를 관측한다. 엔진 계정에서 root journal/worker/config와 다른 인스턴스 data를 변경하지 못하는지 확인한다.
3. 신규 등록·init·start·ready·stop·같은 DB 재시작을 수행한다. unit/InvocationID/PID/cgroup/nodeInstanceId와 report를 같은 시도로 대조한다.
4. 미초기화·중단된 init·변조된 입력·소비한 attempt의 직접 `systemctl start`가 거절되는지 확인한다. 다른 unit/PID에 종료 신호를 보내지 않는지 확인한다.
5. 초기화 중 CLI 중단·서비스 startup 실패·종료 timeout에서 UNKNOWN/잠금/원자료를 보존하고 중복 JVM을 만들지 않는지 확인한다.
6. SSH 종료 뒤 같은 시도의 지속 동작, 정상 종료 이후 cgroup/PID/포트 부재, DB 잠금 해제를 확인한다. 재부팅·강제 종료는 별도 시험이며 정상 stop 결과로 대신하지 않는다.
7. 비공개 전달본으로 순차 설치·계정 승인·이름 기반 운용의 편의성을 관측한다. 기술 동작과 사람 과업 결과는 분리하고 키·원문 작업 폴더는 반환하지 않는다.

검증되지 않은 사례를 PASS로 채우지 않는다. 공개 결과에는 비밀·사용자 경로·인증정보를 넣지 않으며 원자료는 시험 장비의 제한된 폴더에 보관한다. 기존 검증 기록의 결과를 새 실행의 근거로 바꾸지 않는다.

## 참고

- [systemd.service](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html): `Type=exec`, `Restart`와 서비스 시작 의미.
- [systemd.kill](https://www.freedesktop.org/software/systemd/man/latest/systemd.kill.html): `KillMode=control-group`, `SendSIGKILL=no`와 stop timeout.
- [util-linux setpriv 명세](https://kernel.googlesource.com/pub/scm/utils/util-linux/util-linux/+/refs/tags/v2.41.1/sys-utils/setpriv.1.adoc): UID/GID·보조 그룹·capability·no_new_privs 전환과 실패 시 실행 거부.
