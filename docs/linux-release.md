# GitHub Releases에서 Linux 패키지 설치

대상은 Ubuntu 24.04 LTS x86_64/glibc, systemd 환경이다. 공개 시험판이 게시되면 [BXDL Releases](https://github.com/bwg-loonshots/bxdl/releases)에서 **원하는 태그의 설치 명령**을 복사한다. 첫 공개 태그와 배포 공개키는 발급 대기 중이다.

## 처음 설치할 때

일반 사용자 계정에서 `sh`, `curl`, `python3`와 Ed25519 검증을 지원하는 OpenSSL 3, `sha256sum`을 준비한다. 관리 CLI와 NIGO RC.4, Linux용 Temurin 21 JRE는 미리 빌드한 파일로 받는다. 배포 담당자가 별도로 신뢰할 수 있는 경로로 제공한 Ed25519 공개키 PEM을 준비한다. 릴리스 페이지의 설치 명령에는 태그와 설치 스크립트 SHA-256이 실제 값으로 들어간다. 릴리스 페이지 밖에서 스크립트 해시를 확인하려면 같은 태그의 `SHA256SUMS`와 `SHA256SUMS.sig`를 받아 아래 방법을 사용한다.

아래 예시는 같은 절차를 환경변수로 표시한다. `TAG`와 `INSTALLER_SHA256`은 선택한 릴리스의 값, `BXDL_TRUST_KEY`는 신뢰한 공개키의 절대경로다. 새 작업 디렉터리에서 실행한다.

```sh
(
  set -eu
  : "${TAG:?릴리스 태그}" "${INSTALLER_SHA256:?설치 스크립트 SHA-256}" "${BXDL_TRUST_KEY:?신뢰 공개키의 절대경로}"
  curl -fsSL --proto '=https' --proto-redir '=https' \
    "https://github.com/bwg-loonshots/bxdl/releases/download/$TAG/install-bxdl.sh" -o install-bxdl.sh
  printf '%s  %s\n' "$INSTALLER_SHA256" install-bxdl.sh | sha256sum -c -
  sh ./install-bxdl.sh --version "$TAG" --public-key "$BXDL_TRUST_KEY"
)
```

외부 공개키로 스크립트 자체를 인증할 때는 실행 전에 다음을 확인한다. 파일 목록의 서명, 태그, 설치 스크립트 행이 모두 일치해야 한다.

```sh
(
  set -eu
  : "${TAG:?릴리스 태그}" "${BXDL_TRUST_KEY:?신뢰 공개키의 절대경로}"
  url="https://github.com/bwg-loonshots/bxdl/releases/download/$TAG"
  for asset in SHA256SUMS SHA256SUMS.sig install-bxdl.sh; do
    curl -fsSL --proto '=https' --proto-redir '=https' "$url/$asset" -o "$asset"
  done
  openssl pkeyutl -verify -pubin -inkey "$BXDL_TRUST_KEY" -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig
  test "$(head -n 1 SHA256SUMS)" = "# BXDL release $TAG"
  grep -E '^[0-9a-f]{64}  install-bxdl[.]sh$' SHA256SUMS | sha256sum --check --strict -
  sh ./install-bxdl.sh --version "$TAG" --public-key "$BXDL_TRUST_KEY"
)
```

스크립트는 태그가 적힌 파일 목록의 서명을 확인하고 다운로드한 각 파일의 SHA-256을 대조한다. 검증한 실행기로 CLI 번들과 노드 패키지의 내부 서명도 확인한다. 관리 CLI 진입점은 `~/.local/bin/bxdl`, 노드 설치에 사용할 공급물은 `~/.local/share/BXDL-releases/<태그>/runtime-package/`에 준비한다. `~/.local/bin`을 PATH에 추가하거나 출력된 절대경로를 사용한다. 동일 태그로 다시 실행하면 보관된 런타임을 재검증한다. 경로 충돌·검증 실패가 표시되면 해당 경로를 확인한다.

- **파일만 검증**하려면 `--verify-only`를 추가한다.
- **다운로드를 마친 파일로 설치**하려면 같은 태그의 release asset 전체를 한 디렉터리에 모으고 `--from-dir <디렉터리>`를 추가한다. 이때도 동일한 신뢰 공개키를 사용한다.
- **관리자 실행기도 배치**하려면 일반 사용자로 실행하는 위 명령에 `--install-helper`를 추가한다. 시스템 경로 `/usr/local/libexec/bxdl/bxdl`을 준비하는 단계에서 sudo 권한이 필요하다. 옵션을 생략하면 스크립트가 나중에 실행할 명령을 출력한다. 이미 다른 버전의 시스템 실행기가 있으면 설치를 중단하므로 기존 노드의 사용 여부를 확인한다.

## 노드 설정과 실행

설치 마지막에 출력된 `setup new` 명령에는 내려받은 runtime과 JAR 경로가 들어 있다. 운영자가 제공한 NIGO native 설정·체인·validator/TLS 자료를 준비한 뒤 노드 이름을 선택하고 실행한다. 공개 패키지에서 고객용 키나 네트워크 가입 권한을 제공하지 않는다. 이후 단계는 다음 순서다.

```sh
bxdl setup show local
bxdl setup apply local
sudo -v
bxdl init local --confirm-initialize
bxdl start local
bxdl status local --wait
```

설치로 DB나 서비스를 자동으로 시작하지 않는다. 상태와 정상 종료 방법은 [Linux 설치·운용](./linux-operations.md)을 따른다. 기존 노드에서 **관리 CLI만 업데이트**할 때는 별도 CLI 번들과 [관리 CLI 업데이트 절차](./management-cli.md)를 사용한다.
