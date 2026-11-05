# needle documentation
[linux]
@deploy host='prod': build
    rsync -a build/ host:/srv/needle/

URL := 'needle'
alias ship := deploy # needle
